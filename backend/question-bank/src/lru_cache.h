#ifndef QBANK_BACKEND_QUESTION_BANK_SRC_LRU_CACHE_H_
#define QBANK_BACKEND_QUESTION_BANK_SRC_LRU_CACHE_H_

#include <cstddef>
#include <cstdint>
#include <list>
#include <mutex>
#include <string>
#include <unordered_map>

namespace qbank {

// Bounded, thread-safe LRU cache for serialized cache values.
//
// The cache stores opaque byte strings so the same instance can back
// questions, chapters and metadata.  A capacity of zero behaves like a
// one-entry cache to keep the service functional even if the capacity knob is
// misconfigured.
class LruCache {
 public:
  struct Stats {
    std::uint64_t hits = 0;
    std::uint64_t misses = 0;
    std::uint64_t evictions = 0;
    std::uint64_t puts = 0;
  };

  explicit LruCache(std::size_t capacity);

  // Returns true and copies the value when the key is present.  A hit marks
  // the entry as most recently used.
  bool Get(const std::string& key, std::string* value);

  // Inserts or updates an entry.  Inserting beyond the capacity evicts the
  // least recently used entry.
  void Put(std::string key, std::string value);

  void Erase(const std::string& key);

  // Drops all entries.  Counters are retained so hit-ratio dashboards stay
  // meaningful after an invalidation.
  void Clear();

  std::size_t size() const;

  std::size_t capacity() const;

  Stats stats() const;

 private:
  struct Entry {
    std::string key;
    std::string value;
  };

  using EntryList = std::list<Entry>;

  mutable std::mutex mutex_;
  std::size_t capacity_;
  EntryList entries_;
  std::unordered_map<std::string, EntryList::iterator> index_;
  Stats stats_;
};

}  // namespace qbank

#endif  // QBANK_BACKEND_QUESTION_BANK_SRC_LRU_CACHE_H_
