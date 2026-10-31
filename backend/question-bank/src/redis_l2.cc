#include "backend/question-bank/src/redis_l2.h"

#include <utility>

namespace qbank {

std::optional<std::string> InMemoryCacheBackend::Get(const std::string& key) {
  Entry* entry = FindLive(key);
  if (entry == nullptr) {
    return std::nullopt;
  }
  return entry->value;
}

std::vector<std::optional<std::string>> InMemoryCacheBackend::MGet(
    const std::vector<std::string>& keys) {
  std::vector<std::optional<std::string>> values;
  values.reserve(keys.size());
  for (const auto& key : keys) {
    values.push_back(Get(key));
  }
  return values;
}

void InMemoryCacheBackend::Set(const std::string& key, std::string value,
                               std::int64_t ttl_sec) {
  Entry entry;
  entry.value = std::move(value);
  if (ttl_sec > 0) {
    entry.has_expiration = true;
    entry.expires_at =
        std::chrono::steady_clock::now() + std::chrono::seconds(ttl_sec);
  }
  entries_[key] = std::move(entry);
}

void InMemoryCacheBackend::Del(const std::string& key) { entries_.erase(key); }

InMemoryCacheBackend::Entry* InMemoryCacheBackend::FindLive(
    const std::string& key) {
  auto iter = entries_.find(key);
  if (iter == entries_.end()) {
    return nullptr;
  }
  if (iter->second.has_expiration &&
      std::chrono::steady_clock::now() >= iter->second.expires_at) {
    entries_.erase(iter);
    return nullptr;
  }
  return &iter->second;
}

RedisL2::RedisL2(const Settings& settings,
                 std::unique_ptr<CacheBackend> backend)
    : settings_(settings), backend_(std::move(backend)) {}

std::string RedisL2::QuestionKey(const std::string& question_id) {
  return "qbank:question:" + question_id;
}

std::string RedisL2::ChapterKey(const std::string& chapter_id) {
  return "qbank:chapter:" + chapter_id + ":questions";
}

std::string RedisL2::MetaKey(const std::string& question_id) {
  return "qbank:meta:" + question_id;
}

std::optional<std::string> RedisL2::GetQuestion(
    const std::string& question_id) {
  return backend_->Get(QuestionKey(question_id));
}

std::vector<std::optional<std::string>> RedisL2::MGetQuestions(
    const std::vector<std::string>& question_ids) {
  std::vector<std::string> keys;
  keys.reserve(question_ids.size());
  for (const auto& id : question_ids) {
    keys.push_back(QuestionKey(id));
  }
  return backend_->MGet(keys);
}

void RedisL2::PutQuestion(const std::string& question_id,
                          const std::string& value) {
  backend_->Set(QuestionKey(question_id), value, settings_.l2_question_ttl_sec);
}

std::optional<std::string> RedisL2::GetChapter(const std::string& chapter_id) {
  return backend_->Get(ChapterKey(chapter_id));
}

void RedisL2::PutChapter(const std::string& chapter_id,
                         const std::string& value) {
  backend_->Set(ChapterKey(chapter_id), value, settings_.l2_chapter_ttl_sec);
}

std::optional<std::string> RedisL2::GetMeta(const std::string& question_id) {
  return backend_->Get(MetaKey(question_id));
}

std::vector<std::optional<std::string>> RedisL2::MGetMetas(
    const std::vector<std::string>& question_ids) {
  std::vector<std::string> keys;
  keys.reserve(question_ids.size());
  for (const auto& id : question_ids) {
    keys.push_back(MetaKey(id));
  }
  return backend_->MGet(keys);
}

void RedisL2::PutMeta(const std::string& question_id,
                      const std::string& value) {
  backend_->Set(MetaKey(question_id), value, settings_.l2_meta_ttl_sec);
}

void RedisL2::Invalidate(const std::string& question_id) {
  backend_->Del(QuestionKey(question_id));
  backend_->Del(MetaKey(question_id));
}

}  // namespace qbank
