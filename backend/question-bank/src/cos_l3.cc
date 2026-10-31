#include "backend/question-bank/src/cos_l3.h"

#include <algorithm>
#include <utility>

namespace qbank {

void InMemoryCosStore::Put(std::string key, std::string value,
                           std::int64_t updated_at_epoch_ms) {
  std::lock_guard<std::mutex> lock(mutex_);
  Object object;
  object.value = std::move(value);
  object.updated_at_epoch_ms = updated_at_epoch_ms;
  objects_[std::move(key)] = std::move(object);
}

std::optional<std::string> InMemoryCosStore::Fetch(const std::string& key) {
  std::lock_guard<std::mutex> lock(mutex_);
  auto iter = objects_.find(key);
  if (iter == objects_.end()) {
    return std::nullopt;
  }
  return iter->second.value;
}

std::int64_t InMemoryCosStore::updated_at_epoch_ms(const std::string& key) const {
  std::lock_guard<std::mutex> lock(mutex_);
  auto iter = objects_.find(key);
  return iter == objects_.end() ? 0 : iter->second.updated_at_epoch_ms;
}

namespace cos {

std::string QuestionKey(const std::string& question_id) {
  return "qbank/cos/question/" + question_id + ".pb";
}

std::string MetaKey(const std::string& question_id) {
  return "qbank/cos/meta/" + question_id + ".pb";
}

std::string ChapterKey(const std::string& chapter_id) {
  return "qbank/cos/chapter/" + chapter_id + ".txt";
}

}  // namespace cos

CosL3::CosL3(const Settings& settings,
             std::shared_ptr<FetcherBackend> backend)
    : settings_(settings), backend_(std::move(backend)) {}

std::int64_t CosL3::active_fetches() const {
  std::lock_guard<std::mutex> lock(slot_mutex_);
  return active_fetches_;
}

bool CosL3::AcquireFetchSlot() {
  std::unique_lock<std::mutex> lock(slot_mutex_);
  const auto timeout = std::chrono::milliseconds(
      std::max(1, settings_.cos_timeout_ms));
  if (!slot_cv_.wait_for(lock, timeout, [this] {
        return active_fetches_ < settings_.cos_singleflight_max;
      })) {
    return false;
  }
  ++active_fetches_;
  return true;
}

void CosL3::ReleaseFetchSlot() {
  {
    std::lock_guard<std::mutex> lock(slot_mutex_);
    --active_fetches_;
  }
  slot_cv_.notify_one();
}

std::optional<std::string> CosL3::ReadStale(const std::string& key) {
  std::lock_guard<std::mutex> lock(mutex_);
  auto iter = stale_.find(key);
  if (iter == stale_.end()) {
    return std::nullopt;
  }
  // Only serve a stale copy within qbank.cos.stale_serve_max_sec.
  const auto age = std::chrono::duration_cast<std::chrono::seconds>(
      std::chrono::steady_clock::now() - iter->second.fetched_at);
  if (age.count() > settings_.cos_stale_serve_max_sec) {
    return std::nullopt;
  }
  return iter->second.value;
}

void CosL3::PublishStale(const std::string& key, const std::string& value) {
  std::lock_guard<std::mutex> lock(mutex_);
  stale_[key] = StaleEntry{value, std::chrono::steady_clock::now()};
}

std::optional<std::string> CosL3::Fetch(const std::string& key) {
  std::shared_ptr<Flight> flight;
  bool leader = false;
  {
    std::lock_guard<std::mutex> lock(mutex_);
    auto iter = flights_.find(key);
    if (iter == flights_.end()) {
      flight = std::make_shared<Flight>();
      flights_[key] = flight;
      leader = true;
    } else {
      flight = iter->second;
    }
  }

  if (!leader) {
    // Join the in-flight fetch.  Followers never touch the origin themselves.
    std::unique_lock<std::mutex> lock(flight->mutex);
    const auto timeout = std::chrono::milliseconds(
        std::max(1, settings_.cos_timeout_ms));
    if (!flight->cv.wait_for(lock, timeout,
                             [&flight] { return flight->done; })) {
      // The leader is still busy; serve a stale copy if one is recent enough.
      return ReadStale(key);
    }
    return flight->value;
  }

  std::optional<std::string> value;
  if (AcquireFetchSlot()) {
    value = backend_->Fetch(key);
    ReleaseFetchSlot();
  }
  if (value.has_value()) {
    PublishStale(key, *value);
  } else {
    value = ReadStale(key);
  }

  {
    std::lock_guard<std::mutex> lock(flight->mutex);
    flight->value = value;
    flight->done = true;
  }
  flight->cv.notify_all();

  {
    std::lock_guard<std::mutex> lock(mutex_);
    flights_.erase(key);
  }
  return value;
}

}  // namespace qbank
