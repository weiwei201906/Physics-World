#include "backend/question-bank/src/lru_cache.h"

#include <algorithm>
#include <utility>

namespace qbank {

LruCache::LruCache(std::size_t capacity)
    : capacity_(std::max<std::size_t>(1, capacity)) {}

bool LruCache::Get(const std::string& key, std::string* value) {
  std::lock_guard<std::mutex> lock(mutex_);
  auto iter = index_.find(key);
  if (iter == index_.end()) {
    ++stats_.misses;
    return false;
  }
  ++stats_.hits;
  // Splice the entry to the front of the recency list.
  entries_.splice(entries_.begin(), entries_, iter->second);
  if (value != nullptr) {
    *value = iter->second->value;
  }
  return true;
}

void LruCache::Put(std::string key, std::string value) {
  std::lock_guard<std::mutex> lock(mutex_);
  ++stats_.puts;

  auto iter = index_.find(key);
  if (iter != index_.end()) {
    iter->second->value = std::move(value);
    entries_.splice(entries_.begin(), entries_, iter->second);
    return;
  }

  entries_.push_front(Entry{std::move(key), std::move(value)});
  index_[entries_.front().key] = entries_.begin();

  while (entries_.size() > capacity_) {
    index_.erase(entries_.back().key);
    entries_.pop_back();
    ++stats_.evictions;
  }
}

void LruCache::Erase(const std::string& key) {
  std::lock_guard<std::mutex> lock(mutex_);
  auto iter = index_.find(key);
  if (iter == index_.end()) {
    return;
  }
  entries_.erase(iter->second);
  index_.erase(iter);
}

void LruCache::Clear() {
  std::lock_guard<std::mutex> lock(mutex_);
  entries_.clear();
  index_.clear();
}

std::size_t LruCache::size() const {
  std::lock_guard<std::mutex> lock(mutex_);
  return entries_.size();
}

std::size_t LruCache::capacity() const {
  std::lock_guard<std::mutex> lock(mutex_);
  return capacity_;
}

LruCache::Stats LruCache::stats() const {
  std::lock_guard<std::mutex> lock(mutex_);
  return stats_;
}

}  // namespace qbank
