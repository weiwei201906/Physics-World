#include "backend/question-bank/src/grpc_server.h"

#include <atomic>
#include <cstdio>
#include <functional>
#include <memory>
#include <string>
#include <thread>
#include <utility>
#include <vector>

#include "backend/question-bank/qbank.grpc.pb.h"
#include "backend/question-bank/src/bridge.h"
#include "backend/question-bank/src/service.h"
#include "grpcpp/grpcpp.h"

namespace qbank {
namespace {

grpc::Status ToGrpcStatus(const Status& status) {
  switch (status.code) {
    case StatusCode::kOk:
      return grpc::Status::OK;
    case StatusCode::kNotFound:
      return grpc::Status(grpc::StatusCode::NOT_FOUND, status.message);
    case StatusCode::kInvalidArgument:
      return grpc::Status(grpc::StatusCode::INVALID_ARGUMENT, status.message);
    case StatusCode::kUnavailable:
      return grpc::Status(grpc::StatusCode::UNAVAILABLE, status.message);
  }
  return grpc::Status(grpc::StatusCode::INTERNAL, status.message);
}

// Completion-queue tag.  Every asynchronous operation carries one of these.
class Tag {
 public:
  virtual ~Tag() = default;
  virtual void Proceed(bool ok) = 0;
};

}  // namespace

class QuestionBankGrpcServer::Impl {
 public:
  Impl(const Settings& settings, Service* service, Bridge* bridge);
  ~Impl();

  Impl(const Impl&) = delete;
  Impl& operator=(const Impl&) = delete;

  bool Start();
  void Stop();

  grpc::ServerCompletionQueue* cq() const { return cq_; }
  Service* service() const { return service_; }
  bool EnqueueFiber(std::function<void()> task) {
    return bridge_->Enqueue(std::move(task));
  }

 private:
  void StartGetQuestion();
  void StartBatchGetQuestions();
  void StartChapterQuestions();
  void StartQuestionMeta();
  void StartRandomQuestions();
  void RunCompletionQueue();

  Settings settings_;
  Service* service_;
  Bridge* bridge_;
  v1::QuestionBank::AsyncService async_service_;
  grpc::ServerCompletionQueue* cq_ = nullptr;
  std::unique_ptr<grpc::Server> server_;
  std::thread cq_thread_;
  int selected_port_ = 0;
  std::atomic<bool> stopped_{false};
};

namespace {

// One in-flight unary call.  The request handler runs on the bridge; the
// completion-queue thread only performs bookkeeping.
template <typename Request, typename Response>
class UnaryCall final : public Tag {
 public:
  using RequestFn = std::function<void(
      grpc::ServerContext*, const Request*,
      grpc::ServerAsyncResponseWriter<Response>*, grpc::ServerCompletionQueue*,
      void*)>;
  using Handler = std::function<Status(const Request&, Response*)>;
  using NextFn = std::function<void()>;

  UnaryCall(QuestionBankGrpcServer::Impl* server, RequestFn request_fn,
            Handler handler, NextFn next)
      : server_(server),
        request_fn_(std::move(request_fn)),
        handler_(std::move(handler)),
        next_(std::move(next)) {}

  void Start() {
    request_fn_(&context_, &request_, &writer_, server_->cq(), this);
  }

  void Proceed(bool ok) override {
    if (state_ == State::kAwaitingFinish) {
      delete this;
      return;
    }
    if (!ok) {
      // Call cancelled (most likely during shutdown).
      delete this;
      return;
    }

    // Keep exactly one pending request per method so new calls are always
    // accepted while the current one is being processed.
    next_();
    state_ = State::kAwaitingFinish;

    if (!server_->EnqueueFiber([this] {
          Response response;
          Status status = handler_(request_, &response);
          if (!status.ok()) {
            response.Clear();
          }
          writer_.Finish(response, ToGrpcStatus(status), this);
        })) {
      Response response;
      writer_.Finish(
          grpc::Status(grpc::StatusCode::RESOURCE_EXHAUSTED,
                       "qbank fiber bridge queue is full"),
          this);
    }
  }

 private:
  enum class State { kAwaitingRequest, kAwaitingFinish };

  QuestionBankGrpcServer::Impl* server_;
  RequestFn request_fn_;
  Handler handler_;
  NextFn next_;
  grpc::ServerContext context_;
  Request request_;
  grpc::ServerAsyncResponseWriter<Response> writer_{&context_};
  State state_ = State::kAwaitingRequest;
};

// Streaming GetChapterQuestions.  Only one Write is in flight at a time; the
// response vector is produced on the bridge and then drained by the
// completion-queue thread.
class ChapterQuestionsCall final : public Tag {
 public:
  using RequestFn = std::function<void(
      grpc::ServerContext*, const v1::GetChapterQuestionsRequest*,
      grpc::ServerAsyncWriter<v1::Question>*, grpc::ServerCompletionQueue*,
      void*)>;

  ChapterQuestionsCall(QuestionBankGrpcServer::Impl* server,
                       RequestFn request_fn, std::function<void()> next)
      : server_(server),
        request_fn_(std::move(request_fn)),
        next_(std::move(next)) {}

  void Start() {
    request_fn_(&context_, &request_, &writer_, server_->cq(), this);
  }

  void Proceed(bool ok) override {
    switch (state_) {
      case State::kAwaitingRequest:
        if (!ok) {
          delete this;
          return;
        }
        next_();
        state_ = State::kPreparing;
        if (!server_->EnqueueFiber([this] { ProduceQuestions(); })) {
          state_ = State::kAwaitingFinish;
          writer_.Finish(
              grpc::Status(grpc::StatusCode::RESOURCE_EXHAUSTED,
                           "qbank fiber bridge queue is full"),
              this);
        }
        return;
      case State::kAwaitingWrite:
        if (!ok) {
          state_ = State::kAwaitingFinish;
          writer_.Finish(
              grpc::Status(grpc::StatusCode::CANCELLED,
                           "qbank stream write failed"),
              this);
          return;
        }
        if (next_index_ < questions_.size()) {
          writer_.Write(questions_[next_index_++], this);
        } else {
          state_ = State::kAwaitingFinish;
          writer_.Finish(grpc::Status::OK, this);
        }
        return;
      case State::kAwaitingFinish:
        delete this;
        return;
      case State::kPreparing:
        // No tag is expected while the bridge produces the response.
        return;
    }
  }

 private:
  enum class State { kAwaitingRequest, kPreparing, kAwaitingWrite, kAwaitingFinish };

  void ProduceQuestions() {
    Status status = server_->service()->GetChapterQuestions(
        request_.chapter_id(), request_.limit(), &questions_);
    if (!status.ok()) {
      state_ = State::kAwaitingFinish;
      writer_.Finish(ToGrpcStatus(status), this);
      return;
    }
    if (questions_.empty()) {
      state_ = State::kAwaitingFinish;
      writer_.Finish(grpc::Status::OK, this);
      return;
    }
    state_ = State::kAwaitingWrite;
    writer_.Write(questions_[next_index_++], this);
  }

  QuestionBankGrpcServer::Impl* server_;
  RequestFn request_fn_;
  std::function<void()> next_;
  grpc::ServerContext context_;
  v1::GetChapterQuestionsRequest request_;
  grpc::ServerAsyncWriter<v1::Question> writer_{&context_};
  State state_ = State::kAwaitingRequest;
  std::vector<v1::Question> questions_;
  std::size_t next_index_ = 0;
};

}  // namespace

QuestionBankGrpcServer::Impl::Impl(const Settings& settings, Service* service,
                                   Bridge* bridge)
    : settings_(settings), service_(service), bridge_(bridge) {}

QuestionBankGrpcServer::Impl::~Impl() { Stop(); }

bool QuestionBankGrpcServer::Impl::Start() {
  std::string address = settings_.listen_address;
  if (address.empty()) {
    address = "0.0.0.0";
  }
  address += ":" + std::to_string(settings_.port);

  grpc::ServerBuilder builder;
  builder.AddListeningPort(address, grpc::InsecureServerCredentials(),
                           &selected_port_);
  builder.RegisterService(&async_service_);
  cq_ = builder.AddCompletionQueue();
  server_ = builder.BuildAndStart();
  if (server_ == nullptr || cq_ == nullptr ||
      selected_port_ != settings_.port) {
    std::fprintf(stderr, "qbank: failed to listen on %s\n", address.c_str());
    Stop();
    return false;
  }

  StartGetQuestion();
  StartBatchGetQuestions();
  StartChapterQuestions();
  StartQuestionMeta();
  StartRandomQuestions();

  cq_thread_ = std::thread([this] { RunCompletionQueue(); });
  std::fprintf(stderr, "qbank: gRPC server listening on %s\n", address.c_str());
  return true;
}

void QuestionBankGrpcServer::Impl::Stop() {
  if (stopped_.exchange(true)) {
    return;
  }
  if (server_ != nullptr) {
    server_->Shutdown();
  }
  if (cq_ != nullptr) {
    cq_->Shutdown();
  }
  if (cq_thread_.joinable()) {
    cq_thread_.join();
  }
}

void QuestionBankGrpcServer::Impl::RunCompletionQueue() {
  void* tag = nullptr;
  bool ok = false;
  while (cq_->Next(&tag, &ok)) {
    if (tag == nullptr) {
      continue;
    }
    static_cast<Tag*>(tag)->Proceed(ok);
  }
}

void QuestionBankGrpcServer::Impl::StartGetQuestion() {
  auto request_fn = [this](grpc::ServerContext* context,
                           const v1::GetQuestionRequest* request,
                           grpc::ServerAsyncResponseWriter<v1::GetQuestionResponse>* writer,
                           grpc::ServerCompletionQueue* cq, void* tag) {
    async_service_.RequestGetQuestion(context, request, writer, cq, tag);
  };
  auto handler = [this](const v1::GetQuestionRequest& request,
                        v1::GetQuestionResponse* response) {
    return service_->GetQuestion(request.question_id(),
                                 response->mutable_question());
  };
  auto next = [this] { StartGetQuestion(); };
  auto* call = new UnaryCall<v1::GetQuestionRequest, v1::GetQuestionResponse>(
      this, std::move(request_fn), std::move(handler), std::move(next));
  call->Start();
}

void QuestionBankGrpcServer::Impl::StartBatchGetQuestions() {
  auto request_fn = [this](
                        grpc::ServerContext* context,
                        const v1::BatchGetQuestionsRequest* request,
                        grpc::ServerAsyncResponseWriter<
                            v1::BatchGetQuestionsResponse>* writer,
                        grpc::ServerCompletionQueue* cq, void* tag) {
    async_service_.RequestBatchGetQuestions(context, request, writer, cq, tag);
  };
  auto handler = [this](const v1::BatchGetQuestionsRequest& request,
                        v1::BatchGetQuestionsResponse* response) {
    std::vector<std::string> ids;
    ids.reserve(static_cast<std::size_t>(request.question_ids_size()));
    for (const auto& id : request.question_ids()) {
      ids.push_back(id);
    }
    std::vector<v1::Question> questions;
    Status status = service_->BatchGetQuestions(ids, &questions);
    if (status.ok()) {
      for (auto& question : questions) {
        *response->add_questions() = std::move(question);
      }
    }
    return status;
  };
  auto next = [this] { StartBatchGetQuestions(); };
  auto* call =
      new UnaryCall<v1::BatchGetQuestionsRequest, v1::BatchGetQuestionsResponse>(
          this, std::move(request_fn), std::move(handler), std::move(next));
  call->Start();
}

void QuestionBankGrpcServer::Impl::StartChapterQuestions() {
  auto request_fn = [this](
                        grpc::ServerContext* context,
                        const v1::GetChapterQuestionsRequest* request,
                        grpc::ServerAsyncWriter<v1::Question>* writer,
                        grpc::ServerCompletionQueue* cq, void* tag) {
    async_service_.RequestGetChapterQuestions(context, request, writer, cq, tag);
  };
  auto next = [this] { StartChapterQuestions(); };
  auto* call = new ChapterQuestionsCall(this, std::move(request_fn),
                                        std::move(next));
  call->Start();
}

void QuestionBankGrpcServer::Impl::StartQuestionMeta() {
  auto request_fn = [this](
                        grpc::ServerContext* context,
                        const v1::GetQuestionMetaRequest* request,
                        grpc::ServerAsyncResponseWriter<v1::GetQuestionMetaResponse>*
                            writer,
                        grpc::ServerCompletionQueue* cq, void* tag) {
    async_service_.RequestGetQuestionMeta(context, request, writer, cq, tag);
  };
  auto handler = [this](const v1::GetQuestionMetaRequest& request,
                        v1::GetQuestionMetaResponse* response) {
    std::vector<std::string> ids;
    ids.reserve(static_cast<std::size_t>(request.question_ids_size()));
    for (const auto& id : request.question_ids()) {
      ids.push_back(id);
    }
    std::vector<v1::QuestionMeta> metas;
    Status status = service_->GetQuestionMeta(ids, &metas);
    if (status.ok()) {
      for (auto& meta : metas) {
        *response->add_metas() = std::move(meta);
      }
    }
    return status;
  };
  auto next = [this] { StartQuestionMeta(); };
  auto* call =
      new UnaryCall<v1::GetQuestionMetaRequest, v1::GetQuestionMetaResponse>(
          this, std::move(request_fn), std::move(handler), std::move(next));
  call->Start();
}

void QuestionBankGrpcServer::Impl::StartRandomQuestions() {
  auto request_fn = [this](
                        grpc::ServerContext* context,
                        const v1::GetRandomQuestionsRequest* request,
                        grpc::ServerAsyncResponseWriter<
                            v1::GetRandomQuestionsResponse>* writer,
                        grpc::ServerCompletionQueue* cq, void* tag) {
    async_service_.RequestGetRandomQuestions(context, request, writer, cq, tag);
  };
  auto handler = [this](const v1::GetRandomQuestionsRequest& request,
                        v1::GetRandomQuestionsResponse* response) {
    std::vector<v1::Question> questions;
    Status status = service_->GetRandomQuestions(
        request.chapter_id(), request.count(), &questions);
    if (status.ok()) {
      for (auto& question : questions) {
        *response->add_questions() = std::move(question);
      }
    }
    return status;
  };
  auto next = [this] { StartRandomQuestions(); };
  auto* call =
      new UnaryCall<v1::GetRandomQuestionsRequest, v1::GetRandomQuestionsResponse>(
          this, std::move(request_fn), std::move(handler), std::move(next));
  call->Start();
}

QuestionBankGrpcServer::QuestionBankGrpcServer(const Settings& settings,
                                               Service* service,
                                               Bridge* bridge)
    : impl_(std::make_unique<Impl>(settings, service, bridge)) {}

QuestionBankGrpcServer::~QuestionBankGrpcServer() = default;

bool QuestionBankGrpcServer::Start() { return impl_->Start(); }

void QuestionBankGrpcServer::Stop() { impl_->Stop(); }

}  // namespace qbank
