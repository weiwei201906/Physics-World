#ifndef QBANK_BACKEND_QUESTION_BANK_SRC_GRPC_SERVER_H_
#define QBANK_BACKEND_QUESTION_BANK_SRC_GRPC_SERVER_H_

#include <memory>

#include "backend/question-bank/src/settings.h"

namespace qbank {

class Bridge;
class Service;

// Standard gRPC async server for the five QuestionBank methods.
//
// The completion queue itself runs on a dedicated pthread; request handlers
// are dispatched through the injected Bridge (Flare fibers in production,
// std::threads in the local harness).
class QuestionBankGrpcServer {
 public:
  QuestionBankGrpcServer(const Settings& settings, Service* service,
                         Bridge* bridge);
  ~QuestionBankGrpcServer();

  QuestionBankGrpcServer(const QuestionBankGrpcServer&) = delete;
  QuestionBankGrpcServer& operator=(const QuestionBankGrpcServer&) = delete;

  // Binds the port and starts the completion-queue thread.  Returns false if
  // the configured port could not be bound.
  bool Start();

  // Shuts the server and completion queue down and joins the CQ thread.
  void Stop();

 private:
  class Impl;
  std::unique_ptr<Impl> impl_;
};

}  // namespace qbank

#endif  // QBANK_BACKEND_QUESTION_BANK_SRC_GRPC_SERVER_H_
