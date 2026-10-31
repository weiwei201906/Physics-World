#include "backend/question-bank/src/settings.h"

#include <algorithm>
#include <cerrno>
#include <cstdlib>

#ifdef QBANK_WITH_FLARE
#include "gflags/gflags.h"

// Optional command-line overrides for the Flare production build.  Zero means
// "not set", in which case the QBANK_* environment value (or the default)
// applies.
DEFINE_int32(qbank_port, 0, "Override QBANK_PORT / settings.port.");
DEFINE_int32(qbank_grpc_fiber_bridge_queue_size, 0,
             "Override qbank.grpc.fiber_bridge_queue_size.");
DEFINE_int32(qbank_grpc_max_concurrent_fibers, 0,
             "Override qbank.grpc.max_concurrent_fibers.");
DEFINE_int32(qbank_l1_capacity_mb, 0, "Override qbank.l1.capacity_mb.");
DEFINE_int32(qbank_cos_timeout_ms, 0, "Override qbank.cos.timeout_ms.");
#endif

namespace qbank {
namespace {

// 4 KiB is the rough average serialized size of one cached question used to
// turn the MB-based L1 knob into an entry count.
constexpr std::size_t kEstimatedEntryBytes = 4096;

int ReadIntEnv(const char* name, int fallback) {
  const char* raw = std::getenv(name);
  if (raw == nullptr || *raw == '\0') {
    return fallback;
  }
  errno = 0;
  char* end = nullptr;
  long parsed = std::strtol(raw, &end, 10);
  if (errno != 0 || end == raw || *end != '\0') {
    return fallback;
  }
  return static_cast<int>(parsed);
}

std::uint64_t ReadUInt64Env(const char* name, std::uint64_t fallback) {
  const char* raw = std::getenv(name);
  if (raw == nullptr || *raw == '\0') {
    return fallback;
  }
  errno = 0;
  char* end = nullptr;
  unsigned long long parsed = std::strtoull(raw, &end, 10);
  if (errno != 0 || end == raw || *end != '\0') {
    return fallback;
  }
  return static_cast<std::uint64_t>(parsed);
}

std::string ReadStringEnv(const char* name, const std::string& fallback) {
  const char* raw = std::getenv(name);
  if (raw == nullptr || *raw == '\0') {
    return fallback;
  }
  return std::string(raw);
}

#ifdef QBANK_WITH_FLARE
int FlagOrEnv(int flag_value, const char* env_name, int fallback) {
  if (flag_value != 0) {
    return flag_value;
  }
  return ReadIntEnv(env_name, fallback);
}
#endif

std::size_t L1EntriesFromMb(int capacity_mb) {
  std::size_t bytes = static_cast<std::size_t>(std::max(capacity_mb, 0)) *
                      1024 * 1024;
  return std::max<std::size_t>(1, bytes / kEstimatedEntryBytes);
}

}  // namespace

Settings LoadSettingsFromEnv() {
  Settings settings;
  settings.port = ReadIntEnv("QBANK_PORT", settings.port);
  settings.listen_address =
      ReadStringEnv("QBANK_LISTEN_ADDRESS", settings.listen_address);

  settings.grpc_fiber_bridge_queue_size = ReadIntEnv(
      "QBANK_GRPC_FIBER_BRIDGE_QUEUE_SIZE",
      settings.grpc_fiber_bridge_queue_size);
  settings.grpc_max_concurrent_fibers = ReadIntEnv(
      "QBANK_GRPC_MAX_CONCURRENT_FIBERS",
      settings.grpc_max_concurrent_fibers);

  settings.l1_capacity_mb =
      ReadIntEnv("QBANK_L1_CAPACITY_MB", settings.l1_capacity_mb);
  settings.l2_question_ttl_sec =
      ReadIntEnv("QBANK_L2_QUESTION_TTL_SEC", settings.l2_question_ttl_sec);
  settings.l2_chapter_ttl_sec =
      ReadIntEnv("QBANK_L2_CHAPTER_TTL_SEC", settings.l2_chapter_ttl_sec);
  settings.l2_meta_ttl_sec =
      ReadIntEnv("QBANK_L2_META_TTL_SEC", settings.l2_meta_ttl_sec);
  settings.l2_redis_endpoint =
      ReadStringEnv("QBANK_L2_REDIS_ENDPOINT", settings.l2_redis_endpoint);
  settings.l2_redis_timeout_ms =
      ReadIntEnv("QBANK_L2_REDIS_TIMEOUT_MS", settings.l2_redis_timeout_ms);

  settings.cos_timeout_ms =
      ReadIntEnv("QBANK_COS_TIMEOUT_MS", settings.cos_timeout_ms);
  settings.cos_stale_serve_max_sec = ReadIntEnv(
      "QBANK_COS_STALE_SERVE_MAX_SEC", settings.cos_stale_serve_max_sec);
  settings.cos_singleflight_max =
      ReadIntEnv("QBANK_COS_SINGLEFLIGHT_MAX", settings.cos_singleflight_max);
  settings.cos_head_concurrency =
      ReadIntEnv("QBANK_COS_HEAD_CONCURRENCY", settings.cos_head_concurrency);

  settings.flarerpc_address =
      ReadStringEnv("QBANK_FLARERPC_ADDRESS", settings.flarerpc_address);
  settings.flarerpc_port =
      ReadIntEnv("QBANK_FLARERPC_PORT", settings.flarerpc_port);

  settings.random_seed = ReadUInt64Env("QBANK_RANDOM_SEED", settings.random_seed);

#ifdef QBANK_WITH_FLARE
  settings.port = FlagOrEnv(FLAGS_qbank_port, "QBANK_PORT", settings.port);
  settings.grpc_fiber_bridge_queue_size = FlagOrEnv(
      FLAGS_qbank_grpc_fiber_bridge_queue_size,
      "QBANK_GRPC_FIBER_BRIDGE_QUEUE_SIZE",
      settings.grpc_fiber_bridge_queue_size);
  settings.grpc_max_concurrent_fibers = FlagOrEnv(
      FLAGS_qbank_grpc_max_concurrent_fibers,
      "QBANK_GRPC_MAX_CONCURRENT_FIBERS",
      settings.grpc_max_concurrent_fibers);
  settings.l1_capacity_mb =
      FlagOrEnv(FLAGS_qbank_l1_capacity_mb, "QBANK_L1_CAPACITY_MB",
                settings.l1_capacity_mb);
  settings.cos_timeout_ms =
      FlagOrEnv(FLAGS_qbank_cos_timeout_ms, "QBANK_COS_TIMEOUT_MS",
                settings.cos_timeout_ms);
#endif

  settings.l1_capacity_entries = L1EntriesFromMb(settings.l1_capacity_mb);

  // TODO: replace the static defaults above with a snapshot read from
  // `config:qbank` in Redis (polled every qbank.config.refresh_interval_sec),
  // as described in docs/architecture.md section 6.7.

  // Basic sanity: never let a bad env value disable the bridge entirely.
  if (settings.grpc_fiber_bridge_queue_size <= 0) {
    settings.grpc_fiber_bridge_queue_size = 1;
  }
  if (settings.grpc_max_concurrent_fibers <= 0) {
    settings.grpc_max_concurrent_fibers = 1;
  }
  if (settings.port <= 0) {
    settings.port = 15051;
  }
  return settings;
}

}  // namespace qbank
