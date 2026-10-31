#ifndef QBANK_BACKEND_QUESTION_BANK_SRC_SETTINGS_H_
#define QBANK_BACKEND_QUESTION_BANK_SRC_SETTINGS_H_

// Runtime settings for question-bank.
//
// Values follow the `qbank` namespace in docs/architecture.md.  They are
// loaded from QBANK_* environment variables; in the Flare build a small set of
// gflags can override them as well.

#include <cstddef>
#include <cstdint>
#include <string>

namespace qbank {

struct Settings {
  // Defines QBANK_PORT / --qbank_port.
  int port = 15051;
  std::string listen_address = "0.0.0.0";

  // qbank.grpc.fiber_bridge_queue_size / qbank.grpc.max_concurrent_fibers.
  int grpc_fiber_bridge_queue_size = 10000;
  int grpc_max_concurrent_fibers = 5000;

  // qbank.l1.capacity_mb.  L1 is measured in entries internally; the
  // architecture expresses the knob in MB, converted with a fixed estimate.
  int l1_capacity_mb = 256;
  std::size_t l1_capacity_entries = 0;

  // qbank.l2.*.
  int l2_question_ttl_sec = 43200;
  int l2_chapter_ttl_sec = 21600;
  int l2_meta_ttl_sec = 86400;
  std::string l2_redis_endpoint = "127.0.0.1:6379";
  int l2_redis_timeout_ms = 100;

  // qbank.cos.*.
  int cos_timeout_ms = 3000;
  int cos_stale_serve_max_sec = 3600;
  int cos_singleflight_max = 100;
  int cos_head_concurrency = 20;

  // Internal FlareRPC listener (IPC-RPC / QUIC in production).
  std::string flarerpc_address = "127.0.0.1";
  int flarerpc_port = 15061;

  // Seed for deterministic shuffle in GetRandomQuestions.
  std::uint64_t random_seed = 20240101;
};

// Reads QBANK_* variables, falling back to the documented defaults.
Settings LoadSettingsFromEnv();

}  // namespace qbank

#endif  // QBANK_BACKEND_QUESTION_BANK_SRC_SETTINGS_H_
