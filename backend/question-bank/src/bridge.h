#ifndef QBANK_BACKEND_QUESTION_BANK_SRC_BRIDGE_H_
#define QBANK_BACKEND_QUESTION_BANK_SRC_BRIDGE_H_

#include <functional>
#include <memory>

namespace qbank {

// Execution bridge between the gRPC completion-queue thread and the service
// workers.  Production uses Flare fibers; the local development harness uses a
// std::thread pool.
class Bridge {
 public:
  virtual ~Bridge() = default;

  // Schedules a task.  Returns false when the bounded queue is saturated, so
  // the gRPC layer can answer RESOURCE_EXHAUSTED instead of growing unbounded.
  virtual bool Enqueue(std::function<void()> task) = 0;

  virtual void Start() {}

  virtual void Stop() {}
};

}  // namespace qbank

#endif  // QBANK_BACKEND_QUESTION_BANK_SRC_BRIDGE_H_
