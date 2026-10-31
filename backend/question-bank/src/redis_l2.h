#ifndef QBANK_BACKEND_QUESTION_BANK_SRC_REDIS_L2_H_
#define QBANK_BACKEND_QUESTION_BANK_SRC_REDIS_L2_H_

#include <chrono>
#include <cstdint>
#include <map>
#include <memory>
#include <optional>
#include <string>
#include <unordered_map>
#include <vector>

#include "backend/question-bank/src/settings.h"

namespace qbank {

// Cache transport abstraction.  The production implementation speaks RESP to
// the dedicated Redis instance; this milestone ships an in-process stand-in so
// the service logic (MGET / TTL / invalidation) can be exercised everywhere.
// Keeping the interface here matches the CacheBackend boundary in
// docs/architecture.md section 6.7.
class CacheBackend {
 public:
  virtual ~CacheBackend() = default;

  virtual std::optional<std::string> Get(const std::string& key) = 0;

  // Batch read.  The result has the same size and order as `keys`.
  virtual std::vector<std::optional<std::string>> MGet(
      const std::vector<std::string>& keys) = 0;

  // Writes a value with an expiration.  A non-positive TTL means "no
  // expiration".
  virtual void Set(const std::string& key, std::string value,
                   std::int64_t ttl_sec) = 0;

  virtual void Del(const std::string& key) = 0;
};

// Development / test implementation backed by an in-process map.
class InMemoryCacheBackend final : public CacheBackend {
 public:
  std::optional<std::string> Get(const std::string& key) override;
  std::vector<std::optional<std::string>> MGet(
      const std::vector<std::string>& keys) override;
  void Set(const std::string& key, std::string value,
           std::int64_t ttl_sec) override;
  void Del(const std::string& key) override;

 private:
  struct Entry {
    std::string value;
    std::chrono::steady_clock::time_point expires_at;
    bool has_expiration = false;
  };

  Entry* FindLive(const std::string& key);

  std::map<std::string, Entry> entries_;
};

// L2 cache facade.  Owns the qbank key scheme and TTLs documented in
// docs/architecture.md, section 9.5.
class RedisL2 {
 public:
  RedisL2(const Settings& settings, std::unique_ptr<CacheBackend> backend);

  static std::string QuestionKey(const std::string& question_id);
  static std::string ChapterKey(const std::string& chapter_id);
  static std::string MetaKey(const std::string& question_id);

  std::optional<std::string> GetQuestion(const std::string& question_id);
  std::vector<std::optional<std::string>> MGetQuestions(
      const std::vector<std::string>& question_ids);
  void PutQuestion(const std::string& question_id, const std::string& value);

  std::optional<std::string> GetChapter(const std::string& chapter_id);
  void PutChapter(const std::string& chapter_id, const std::string& value);

  std::optional<std::string> GetMeta(const std::string& question_id);
  std::vector<std::optional<std::string>> MGetMetas(
      const std::vector<std::string>& question_ids);
  void PutMeta(const std::string& question_id, const std::string& value);

  // Drops both the question and its metadata after an admin update.
  void Invalidate(const std::string& question_id);

  CacheBackend* backend() { return backend_.get(); }

 private:
  Settings settings_;
  std::unique_ptr<CacheBackend> backend_;
};

}  // namespace qbank

#endif  // QBANK_BACKEND_QUESTION_BANK_SRC_REDIS_L2_H_
