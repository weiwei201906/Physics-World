#ifndef QBANK_BACKEND_QUESTION_BANK_SRC_COS_L3_H_
#define QBANK_BACKEND_QUESTION_BANK_SRC_COS_L3_H_

#include <chrono>
#include <condition_variable>
#include <cstdint>
#include <map>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <unordered_map>

#include "backend/question-bank/src/settings.h"

namespace qbank {

// Object store abstraction.  The production implementation maps to COS
// GetObject / HEAD; this milestone uses an in-process store seeded from the
// repository so the singleflight and stale-serving paths are real code.
class FetcherBackend {
 public:
  virtual ~FetcherBackend() = default;

  // Returns the object payload, or nullopt when the object does not exist or
  // the fetch failed.
  virtual std::optional<std::string> Fetch(const std::string& key) = 0;
};

// In-process COS stand-in populated by the service at construction time.
class InMemoryCosStore final : public FetcherBackend {
 public:
  // Stores an object and its COS metadata timestamp.
  void Put(std::string key, std::string value, std::int64_t updated_at_epoch_ms);

  std::optional<std::string> Fetch(const std::string& key) override;

  std::int64_t updated_at_epoch_ms(const std::string& key) const;

 private:
  struct Object {
    std::string value;
    std::int64_t updated_at_epoch_ms = 0;
  };

  mutable std::mutex mutex_;
  std::map<std::string, Object> objects_;
};

namespace cos {

std::string QuestionKey(const std::string& question_id);
std::string MetaKey(const std::string& question_id);
std::string ChapterKey(const std::string& chapter_id);

}  // namespace cos

// L3 COS fetcher with per-key singleflight, a bounded number of concurrent
// origin fetches, a fetch timeout and stale-serving after an origin failure.
class CosL3 {
 public:
  CosL3(const Settings& settings, std::shared_ptr<FetcherBackend> backend);

  // Fetches `key`, coalescing concurrent callers for the same key.  Returns
  // nullopt when the object is absent and no stale copy is serviceable.
  std::optional<std::string> Fetch(const std::string& key);

  // Number of backend fetches currently executing.
  std::int64_t active_fetches() const;

 private:
  struct Flight {
    std::mutex mutex;
    std::condition_variable cv;
    bool done = false;
    std::optional<std::string> value;
  };

  struct StaleEntry {
    std::string value;
    std::chrono::steady_clock::time_point fetched_at;
  };

  std::optional<std::string> ReadStale(const std::string& key);
  void PublishStale(const std::string& key, const std::string& value);
  bool AcquireFetchSlot();
  void ReleaseFetchSlot();

  Settings settings_;
  std::shared_ptr<FetcherBackend> backend_;

  mutable std::mutex mutex_;
  std::unordered_map<std::string, std::shared_ptr<Flight>> flights_;
  std::unordered_map<std::string, StaleEntry> stale_;

  std::mutex slot_mutex_;
  std::condition_variable slot_cv_;
  std::int64_t active_fetches_ = 0;
};

}  // namespace qbank

#endif  // QBANK_BACKEND_QUESTION_BANK_SRC_COS_L3_H_
