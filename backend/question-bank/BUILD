# BUILD for the question-bank service.
#
# The external contract is the standard gRPC service in
# //contracts/proto:qbank_proto.  Runtime handler execution uses Tencent Flare
# fibers (//flare/fiber:fiber); the process is bootstrapped by flare::Start
# from //flare:init.  Production / CI builds therefore need a Flare checkout at
# //flare, exactly as Flare's own example/rpc/BUILD does.

load('//flare/tools/build_rules.bld', 'cc_flare_library')

# gRPC async stubs (qbank.grpc.pb.*) generated from the shared contract.
# This mirrors the protoc + plugin recipe used by `cc_flare_library`; only the
# code generator differs (grpc_cpp_plugin instead of the flare_rpc plugin).
gen_rule(
  name = 'qbank_grpc_gen',
  srcs = ['//contracts/proto:qbank.proto'],
  deps = ['//contracts/proto:qbank_proto'],
  outs = [
    'qbank.grpc.pb.h',
    'qbank.grpc.pb.cc',
  ],
  cmd = 'protoc --proto_path=contracts/proto --grpc_out=$TARGET_DIR '
        '--plugin=protoc-gen-grpc=`which grpc_cpp_plugin` qbank.proto',
  cmd_name = 'GRPC_STUBS',
)

cc_library(
  name = 'qbank_grpc_stubs',
  hdrs = ['qbank.grpc.pb.h'],
  srcs = ['qbank.grpc.pb.cc'],
  deps = [
    ':qbank_grpc_gen',
    '//contracts/proto:qbank_proto',
    '//thirdparty/grpc:grpc++',
  ],
)

# Core logic: settings, repository, caches and the 5-method service.  No Flare
# dependency, so it is also exercised by the local gtest harness.
cc_library(
  name = 'qbank_core',
  hdrs = [
    'src/cos_l3.h',
    'src/lru_cache.h',
    'src/redis_l2.h',
    'src/repository.h',
    'src/service.h',
    'src/settings.h',
  ],
  srcs = [
    'src/cos_l3.cc',
    'src/lru_cache.cc',
    'src/redis_l2.cc',
    'src/repository.cc',
    'src/service.cc',
    'src/settings.cc',
  ],
  deps = [
    '//contracts/proto:qbank_proto',
  ],
  defs = ['QBANK_WITH_FLARE'],
)

# Bounded gRPC -> Flare fiber bridge.
cc_library(
  name = 'qbank_bridge_flare',
  hdrs = [
    'src/bridge.h',
    'src/flare_bridge.h',
  ],
  srcs = [
    'src/bridge.cc',
    'src/flare_bridge.cc',
  ],
  deps = [
    '//flare/base:function',
    '//flare/fiber:fiber',
  ],
  defs = ['QBANK_WITH_FLARE'],
)

# Standard gRPC async server whose handlers run on Flare fibers.
cc_library(
  name = 'qbank_grpc',
  hdrs = ['src/grpc_server.h'],
  srcs = ['src/grpc_server.cc'],
  deps = [
    ':qbank_bridge_flare',
    ':qbank_core',
    ':qbank_grpc_stubs',
    '//flare/fiber:condition_variable',
    '//flare/fiber:fiber',
    '//flare/fiber:mutex',
    '//thirdparty/grpc:grpc++',
    '//thirdparty/protobuf:protobuf',
  ],
  defs = ['QBANK_WITH_FLARE'],
)

cc_binary(
  name = 'qbank_server',
  srcs = ['src/main.cc'],
  deps = [
    ':qbank_bridge_flare',
    ':qbank_core',
    ':qbank_grpc',
    '//flare:init',
    '//flare/base/net:endpoint',
    '//flare/fiber:fiber',
    '//flare/init:override_flag',
    '//flare/rpc:rpc',
    '//thirdparty/gflags:gflags',
    '//thirdparty/grpc:grpc++',
    '//thirdparty/protobuf:protobuf',
  ],
  defs = ['QBANK_WITH_FLARE'],
)

# Tests deliberately cover only the Flare-free core (LRU, repository, service
# validation and deterministic random) so they can also run with local_build.sh.
cc_test(
  name = 'qbank_tests',
  srcs = ['tests/qbank_tests.cc'],
  deps = [
    ':qbank_core',
    '//contracts/proto:qbank_proto',
    '//thirdparty/googletest:gtest',
    '//thirdparty/googletest:gtest_main',
  ],
)

# NOTE: the internal FlareRPC bindings referenced by src/main.cc are a
# skeleton today (a //flare/rpc Server listening on the IPC port with no
# registered service).  Once an internal contract exists, add it here with
# `cc_flare_library` (the same pattern as Flare's example/rpc/BUILD) and
# register the generated service in StartInternalFlareRpc().
