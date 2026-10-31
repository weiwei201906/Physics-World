#ifndef QBANK_BACKEND_QUESTION_BANK_SRC_FLARE_BRIDGE_H_
#define QBANK_BACKEND_QUESTION_BANK_SRC_FLARE_BRIDGE_H_

#include <memory>

#include "backend/question-bank/src/bridge.h"

namespace qbank {

struct Settings;

// Production bridge.  Queues at most qbank.grpc.fiber_bridge_queue_size
// pending tasks and runs at most qbank.grpc.max_concurrent_fibers of them at
// a time.  Must be created and used inside a Flare runtime.
std::unique_ptr<Bridge> CreateFlareFiberBridge(const Settings& settings);

}  // namespace qbank

#endif  // QBANK_BACKEND_QUESTION_BANK_SRC_FLARE_BRIDGE_H_
