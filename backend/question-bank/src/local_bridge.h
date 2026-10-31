#ifndef QBANK_BACKEND_QUESTION_BANK_SRC_LOCAL_BRIDGE_H_
#define QBANK_BACKEND_QUESTION_BANK_SRC_LOCAL_BRIDGE_H_

#include <memory>

#include "backend/question-bank/src/bridge.h"

namespace qbank {

struct Settings;

// Development-only fallback used by local_build.sh.  It mirrors the Flare
// bridge's queue and concurrency knobs with a fixed std::thread pool and is
// never linked into the production Blade target.
std::unique_ptr<Bridge> CreateLocalThreadBridge(const Settings& settings);

}  // namespace qbank

#endif  // QBANK_BACKEND_QUESTION_BANK_SRC_LOCAL_BRIDGE_H_
