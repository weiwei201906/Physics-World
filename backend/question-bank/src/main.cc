#include <atomic>
#include <chrono>
#include <csignal>
#include <cstdio>
#include <memory>
#include <thread>

#include "backend/question-bank/src/grpc_server.h"
#include "backend/question-bank/src/service.h"
#include "backend/question-bank/src/settings.h"

#if defined(QBANK_WITH_FLARE)
#include "backend/question-bank/src/flare_bridge.h"
#include "flare/base/logging.h"
#include "flare/init.h"
#include "flare/init/override_flag.h"

FLARE_OVERRIDE_FLAG(logtostderr, true);
#else
#include "backend/question-bank/src/local_bridge.h"
#endif

namespace qbank {
namespace {

#if defined(QBANK_WITH_FLARE)
int Entry(int argc, char** argv) {
  (void)argc;
  (void)argv;

  Settings settings = LoadSettingsFromEnv();
  Service service(settings);
  auto bridge = CreateFlareFiberBridge(settings);
  bridge->Start();

  QuestionBankGrpcServer server(settings, &service, bridge.get());
  if (!server.Start()) {
    FLARE_LOG_ERROR("Failed to start the qbank gRPC server.");
    return 1;
  }

  flare::WaitForQuitSignal();
  server.Stop();
  bridge->Stop();
  return 0;
}
#else
std::atomic<bool> g_quit{false};

void HandleQuitSignal(int) {
  g_quit.store(true, std::memory_order_release);
}
#endif

}  // namespace
}  // namespace qbank

#if defined(QBANK_WITH_FLARE)
int main(int argc, char** argv) {
  return flare::Start(argc, argv, qbank::Entry);
}
#else
int main(int argc, char** argv) {
  (void)argc;
  (void)argv;

  // Local development harness: no Flare runtime is linked, so handlers run on
  // the std::thread bridge and the process waits for SIGINT / SIGTERM.
  qbank::Settings settings = qbank::LoadSettingsFromEnv();
  qbank::Service service(settings);
  auto bridge = qbank::CreateLocalThreadBridge(settings);

  qbank::QuestionBankGrpcServer server(settings, &service, bridge.get());
  if (!server.Start()) {
    return 1;
  }

  std::signal(SIGINT, qbank::HandleQuitSignal);
  std::signal(SIGTERM, qbank::HandleQuitSignal);
  while (!qbank::g_quit.load(std::memory_order_acquire)) {
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
  }

  server.Stop();
  bridge->Stop();
  std::fprintf(stderr, "qbank: shutdown complete\n");
  return 0;
}
#endif
