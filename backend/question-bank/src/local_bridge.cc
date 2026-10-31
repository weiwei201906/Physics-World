#include "backend/question-bank/src/local_bridge.h"

#include <algorithm>
#include <atomic>
#include <condition_variable>
#include <cstddef>
#include <deque>
#include <functional>
#include <memory>
#include <mutex>
#include <thread>
#include <utility>
#include <vector>

#include "backend/question-bank/src/settings.h"

namespace qbank {
namespace {

class LocalThreadBridge final : public Bridge {
 public:
  explicit LocalThreadBridge(const Settings& settings)
      : queue_size_(static_cast<std::size_t>(
            std::max(1, settings.grpc_fiber_bridge_queue_size))),
        max_concurrent_(static_cast<std::size_t>(
            std::max(1, settings.grpc_max_concurrent_fibers))) {
    // A few OS threads are enough for the local harness; the production
    // bridge honors the full fiber concurrency setting.
    std::size_t worker_count = std::max<std::size_t>(
        2, std::min<std::size_t>(8, std::thread::hardware_concurrency()));
    worker_count = std::min<std::size_t>(worker_count, max_concurrent_);
    threads_.reserve(worker_count);
    for (std::size_t i = 0; i < worker_count; ++i) {
      threads_.emplace_back([this] { WorkerLoop(); });
    }
  }

  ~LocalThreadBridge() override { Stop(); }

  LocalThreadBridge(const LocalThreadBridge&) = delete;
  LocalThreadBridge& operator=(const LocalThreadBridge&) = delete;

  bool Enqueue(std::function<void()> task) override {
    if (task == nullptr || stopping_.load(std::memory_order_acquire)) {
      return false;
    }
    {
      std::lock_guard<std::mutex> lock(mutex_);
      if (pending_ >= queue_size_) {
        return false;
      }
      ++pending_;
      tasks_.push_back(std::move(task));
    }
    cv_.notify_one();
    return true;
  }

  void Stop() override {
    if (stopping_.exchange(true, std::memory_order_acq_rel)) {
      return;
    }
    cv_.notify_all();
    for (auto& thread : threads_) {
      if (thread.joinable()) {
        thread.join();
      }
    }
  }

 private:
  void WorkerLoop() {
    while (true) {
      std::function<void()> task;
      {
        std::unique_lock<std::mutex> lock(mutex_);
        cv_.wait(lock, [this] { return stopping_ || !tasks_.empty(); });
        if (tasks_.empty()) {
          return;
        }
        task = std::move(tasks_.front());
        tasks_.pop_front();
      }
      task();
      {
        std::lock_guard<std::mutex> lock(mutex_);
        --pending_;
      }
    }
  }

  const std::size_t queue_size_;
  const std::size_t max_concurrent_;
  std::mutex mutex_;
  std::condition_variable cv_;
  std::deque<std::function<void()>> tasks_;
  std::vector<std::thread> threads_;
  std::size_t pending_ = 0;
  std::atomic<bool> stopping_{false};
};

}  // namespace

std::unique_ptr<Bridge> CreateLocalThreadBridge(const Settings& settings) {
  return std::make_unique<LocalThreadBridge>(settings);
}

}  // namespace qbank
