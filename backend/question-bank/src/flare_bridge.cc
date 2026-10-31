#include "backend/question-bank/src/flare_bridge.h"

#include <algorithm>
#include <atomic>
#include <cstddef>
#include <functional>
#include <memory>
#include <utility>

#include "backend/question-bank/src/settings.h"
#include "flare/fiber/fiber.h"
#include "flare/fiber/semaphore.h"

namespace qbank {
namespace {

// Bounded handoff from the gRPC completion queue to Flare's fiber scheduler.
//
// The completion queue runs on a plain pthread, so Enqueue() may be called
// outside a fiber.  The explicit scheduling group in Fiber::Attributes is what
// makes StartFiberDetached() usable from that thread: the plain overload
// resolves the current thread's scheduling group, which only exists on flare
// fiber workers.
class FlareFiberBridge final : public Bridge {
 public:
  explicit FlareFiberBridge(const Settings& settings)
      : queue_size_(static_cast<std::size_t>(
            std::max(1, settings.grpc_fiber_bridge_queue_size))),
        max_concurrent_(static_cast<std::size_t>(
            std::max(1, settings.grpc_max_concurrent_fibers))),
        slots_(static_cast<std::ptrdiff_t>(max_concurrent_)) {}

  bool Enqueue(std::function<void()> task) override {
    if (task == nullptr ||
        stopping_.load(std::memory_order_acquire)) {
      return false;
    }

    // Admission control: pending counts queued plus running tasks.
    auto previous = pending_.fetch_add(1, std::memory_order_acq_rel);
    if (previous >= queue_size_) {
      pending_.fetch_sub(1, std::memory_order_acq_rel);
      return false;
    }

    flare::fiber::internal::StartFiberDetached(
        flare::Fiber::Attributes{.scheduling_group = 0},
        [this, task = std::move(task)]() mutable {
          slots_.acquire();
          task();
          slots_.release();
          pending_.fetch_sub(1, std::memory_order_acq_rel);
        });
    return true;
  }

  void Stop() override {
    stopping_.store(true, std::memory_order_release);
  }

 private:
  const std::size_t queue_size_;
  const std::size_t max_concurrent_;
  std::atomic<std::size_t> pending_{0};
  std::atomic<bool> stopping_{false};
  flare::fiber::CountingSemaphore<> slots_;
};

}  // namespace

std::unique_ptr<Bridge> CreateFlareFiberBridge(const Settings& settings) {
  return std::make_unique<FlareFiberBridge>(settings);
}

}  // namespace qbank
