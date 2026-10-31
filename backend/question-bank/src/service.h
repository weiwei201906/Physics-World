#ifndef QBANK_BACKEND_QUESTION_BANK_SRC_SERVICE_H_
#define QBANK_BACKEND_QUESTION_BANK_SRC_SERVICE_H_

#include <cstdint>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#include "backend/question-bank/src/cos_l3.h"
#include "backend/question-bank/src/lru_cache.h"
#include "backend/question-bank/src/redis_l2.h"
#include "backend/question-bank/src/repository.h"
#include "backend/question-bank/src/settings.h"
#include "contracts/proto/qbank.pb.h"

namespace qbank {

enum class StatusCode {
  kOk = 0,
  kNotFound,
  kInvalidArgument,
  kUnavailable,
};

// Transport-neutral result of a service call.  The gRPC layer maps these onto
// canonical status codes; the local harness exercises them directly.
struct Status {
  StatusCode code = StatusCode::kOk;
  std::string message;

  bool ok() const { return code == StatusCode::kOk; }

  static Status Ok() { return Status{}; }

  static Status InvalidArgument(std::string message) {
    return Status{StatusCode::kInvalidArgument, std::move(message)};
  }

  static Status NotFound(std::string message) {
    return Status{StatusCode::kNotFound, std::move(message)};
  }

  static Status Unavailable(std::string message) {
    return Status{StatusCode::kUnavailable, std::move(message)};
  }
};

// Query Service: reads through L1 LRU -> L2 Redis -> L3 COS.
//
// All methods are safe to call concurrently; the caches own their
// synchronization and the repository is immutable after construction.
class Service {
 public:
  explicit Service(const Settings& settings);
  Service(const Settings& settings, std::uint64_t seed);

  Status GetQuestion(const std::string& question_id,
                     v1::Question* question);

  Status BatchGetQuestions(const std::vector<std::string>& question_ids,
                           std::vector<v1::Question>* questions);

  // `limit` <= 0 means "all questions in the chapter".  Positive values are
  // clamped to kMaxQuestionsPerCall before being applied.
  Status GetChapterQuestions(const std::string& chapter_id, int limit,
                             std::vector<v1::Question>* questions);

  Status GetQuestionMeta(const std::vector<std::string>& question_ids,
                         std::vector<v1::QuestionMeta>* metas);

  // `count` <= 0 yields an empty response.  Positive values are clamped to
  // kMaxQuestionsPerCall; the seeded shuffle is deterministic.
  Status GetRandomQuestions(const std::string& chapter_id, int count,
                            std::vector<v1::Question>* questions);

  const Settings& settings() const { return settings_; }

  std::uint64_t seed() const { return repository_.seed(); }

 private:
  Status LoadQuestion(const std::string& question_id,
                      v1::Question* question);
  Status LoadChapterIds(const std::string& chapter_id,
                        std::vector<std::string>* question_ids);
  Status LoadMeta(const std::string& question_id,
                  v1::QuestionMeta* meta);

  Settings settings_;
  Repository repository_;
  std::shared_ptr<InMemoryCosStore> cos_store_;
  LruCache l1_;
  RedisL2 l2_;
  CosL3 l3_;
};

}  // namespace qbank

#endif  // QBANK_BACKEND_QUESTION_BANK_SRC_SERVICE_H_
