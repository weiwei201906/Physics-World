#include "backend/question-bank/src/service.h"

#include <algorithm>
#include <limits>
#include <utility>

namespace qbank {
namespace {

// Upper bound applied to streaming limits and random counts before they reach
// the repository, so a single call can never fan out unbounded.
constexpr int kMaxQuestionsPerCall = 100;

// L1 keys use a local namespace because L1 is process-local and unrelated to
// the Redis key scheme.
std::string L1QuestionKey(const std::string& question_id) {
  return "question:" + question_id;
}

std::string L1ChapterKey(const std::string& chapter_id) {
  return "chapter:" + chapter_id;
}

std::string L1MetaKey(const std::string& question_id) {
  return "meta:" + question_id;
}

std::string EncodeIds(const std::vector<std::string>& ids) {
  std::string payload;
  for (const auto& id : ids) {
    payload.append(id);
    payload.push_back('\n');
  }
  return payload;
}

std::vector<std::string> DecodeIds(const std::string& payload) {
  std::vector<std::string> ids;
  std::string current;
  for (char c : payload) {
    if (c == '\n') {
      if (!current.empty()) {
        ids.push_back(current);
        current.clear();
      }
    } else {
      current.push_back(c);
    }
  }
  if (!current.empty()) {
    ids.push_back(current);
  }
  return ids;
}

v1::Question ToProto(const QuestionRecord& record) {
  v1::Question question;
  question.set_id(record.id);
  question.set_chapter_id(record.chapter_id);
  question.set_difficulty(record.difficulty);

  auto* content = question.mutable_content();
  content->set_stem(record.stem);
  content->set_image_url(record.image_url);
  content->set_explanation(record.explanation);
  for (const auto& option : record.options) {
    auto* proto_option = content->add_options();
    proto_option->set_id(option.id);
    proto_option->set_label(option.label);
    proto_option->set_text(option.text);
  }
  return question;
}

v1::QuestionMeta ToProtoMeta(const QuestionRecord& record) {
  v1::QuestionMeta meta;
  meta.set_id(record.id);
  meta.set_chapter_id(record.chapter_id);
  meta.set_difficulty(record.difficulty);
  meta.set_updated_at_epoch_ms(record.updated_at_epoch_ms);
  return meta;
}

// Fills the COS stand-in with the objects an admin pipeline would have written:
// one question object, one metadata object and one chapter manifest per record.
void SeedCosStore(const Repository& repository, InMemoryCosStore* store) {
  for (const auto& [chapter_id, records] : repository.chapters()) {
    std::vector<std::string> ids;
    ids.reserve(records.size());
    for (const auto& record : records) {
      ids.push_back(record.id);
      store->Put(cos::QuestionKey(record.id), ToProto(record).SerializeAsString(),
                 record.updated_at_epoch_ms);
      store->Put(cos::MetaKey(record.id), ToProtoMeta(record).SerializeAsString(),
                 record.updated_at_epoch_ms);
    }
    store->Put(cos::ChapterKey(chapter_id), EncodeIds(ids),
               records.empty() ? 0 : records.front().updated_at_epoch_ms);
  }
}

}  // namespace

Service::Service(const Settings& settings)
    : Service(settings, settings.random_seed) {}

Service::Service(const Settings& settings, std::uint64_t seed)
    : settings_(settings),
      repository_(seed),
      cos_store_(std::make_shared<InMemoryCosStore>()),
      l1_(settings.l1_capacity_entries),
      l2_(settings, std::make_unique<InMemoryCacheBackend>()),
      l3_(settings, cos_store_) {
  SeedCosStore(repository_, cos_store_.get());
}

Status Service::LoadQuestion(const std::string& question_id,
                             v1::Question* question) {
  if (question_id.empty()) {
    return Status::InvalidArgument("question_id must not be empty");
  }

  std::string bytes;
  if (!l1_.Get(L1QuestionKey(question_id), &bytes)) {
    auto cached = l2_.GetQuestion(question_id);
    if (cached.has_value()) {
      bytes = *cached;
    } else {
      auto fetched = l3_.Fetch(cos::QuestionKey(question_id));
      if (!fetched.has_value()) {
        if (repository_.Find(question_id) != nullptr) {
          return Status::Unavailable("COS fetch failed for question " +
                                     question_id);
        }
        return Status::NotFound("question " + question_id + " not found");
      }
      bytes = *fetched;
      l2_.PutQuestion(question_id, bytes);
    }
    l1_.Put(L1QuestionKey(question_id), bytes);
  }

  v1::Question parsed;
  if (!parsed.ParseFromString(bytes)) {
    return Status::Unavailable("corrupt cached payload for question " +
                               question_id);
  }
  *question = std::move(parsed);
  return Status::Ok();
}

Status Service::LoadChapterIds(const std::string& chapter_id,
                               std::vector<std::string>* question_ids) {
  if (chapter_id.empty()) {
    return Status::InvalidArgument("chapter_id must not be empty");
  }

  std::string bytes;
  if (!l1_.Get(L1ChapterKey(chapter_id), &bytes)) {
    auto cached = l2_.GetChapter(chapter_id);
    if (cached.has_value()) {
      bytes = *cached;
    } else {
      auto fetched = l3_.Fetch(cos::ChapterKey(chapter_id));
      if (!fetched.has_value()) {
        if (repository_.FindChapter(chapter_id) != nullptr) {
          return Status::Unavailable("COS fetch failed for chapter " +
                                     chapter_id);
        }
        return Status::NotFound("chapter " + chapter_id + " not found");
      }
      bytes = *fetched;
      l2_.PutChapter(chapter_id, bytes);
    }
    l1_.Put(L1ChapterKey(chapter_id), bytes);
  }

  *question_ids = DecodeIds(bytes);
  if (question_ids->empty()) {
    if (repository_.FindChapter(chapter_id) == nullptr) {
      return Status::NotFound("chapter " + chapter_id + " not found");
    }
    return Status::Unavailable("empty chapter manifest for " + chapter_id);
  }
  return Status::Ok();
}

Status Service::LoadMeta(const std::string& question_id,
                         v1::QuestionMeta* meta) {
  std::string bytes;
  if (!l1_.Get(L1MetaKey(question_id), &bytes)) {
    auto cached = l2_.GetMeta(question_id);
    if (cached.has_value()) {
      bytes = *cached;
    } else {
      auto fetched = l3_.Fetch(cos::MetaKey(question_id));
      if (!fetched.has_value()) {
        if (repository_.Find(question_id) != nullptr) {
          return Status::Unavailable("COS metadata fetch failed for question " +
                                     question_id);
        }
        return Status::NotFound("question " + question_id + " not found");
      }
      bytes = *fetched;
      l2_.PutMeta(question_id, bytes);
    }
    l1_.Put(L1MetaKey(question_id), bytes);
  }

  v1::QuestionMeta parsed;
  if (!parsed.ParseFromString(bytes)) {
    return Status::Unavailable("corrupt cached metadata for question " +
                               question_id);
  }
  *meta = std::move(parsed);
  return Status::Ok();
}

Status Service::GetQuestion(const std::string& question_id,
                            v1::Question* question) {
  return LoadQuestion(question_id, question);
}

Status Service::BatchGetQuestions(
    const std::vector<std::string>& question_ids,
    std::vector<v1::Question>* questions) {
  questions->clear();
  questions->reserve(question_ids.size());
  for (const auto& question_id : question_ids) {
    v1::Question question;
    Status status = LoadQuestion(question_id, &question);
    if (!status.ok()) {
      return status;
    }
    questions->push_back(std::move(question));
  }
  return Status::Ok();
}

Status Service::GetChapterQuestions(const std::string& chapter_id, int limit,
                                    std::vector<v1::Question>* questions) {
  if (chapter_id.empty()) {
    return Status::InvalidArgument("chapter_id must not be empty");
  }

  std::vector<std::string> ids;
  Status status = LoadChapterIds(chapter_id, &ids);
  if (!status.ok()) {
    return status;
  }

  int take = limit <= 0 ? std::numeric_limits<int>::max()
                        : std::min(limit, kMaxQuestionsPerCall);
  take = std::min<int>(take, static_cast<int>(ids.size()));

  questions->clear();
  questions->reserve(static_cast<std::size_t>(take));
  for (int i = 0; i < take; ++i) {
    v1::Question question;
    status = LoadQuestion(ids[static_cast<std::size_t>(i)], &question);
    if (!status.ok()) {
      return status;
    }
    questions->push_back(std::move(question));
  }
  return Status::Ok();
}

Status Service::GetQuestionMeta(
    const std::vector<std::string>& question_ids,
    std::vector<v1::QuestionMeta>* metas) {
  metas->clear();
  if (question_ids.empty()) {
    return Status::Ok();
  }

  // Batch read the metadata keys first (MGET in Redis); hits are promoted into
  // L1 so LoadMeta does not issue one L2 round trip per key.
  std::vector<std::optional<std::string>> cached = l2_.MGetMetas(question_ids);
  for (std::size_t i = 0; i < question_ids.size(); ++i) {
    if (cached[i].has_value()) {
      l1_.Put(L1MetaKey(question_ids[i]), *cached[i]);
    }
  }

  metas->reserve(question_ids.size());
  for (std::size_t i = 0; i < question_ids.size(); ++i) {
    v1::QuestionMeta meta;
    Status status = LoadMeta(question_ids[i], &meta);
    if (!status.ok()) {
      return status;
    }
    metas->push_back(std::move(meta));
  }
  return Status::Ok();
}

Status Service::GetRandomQuestions(const std::string& chapter_id, int count,
                                   std::vector<v1::Question>* questions) {
  if (chapter_id.empty()) {
    return Status::InvalidArgument("chapter_id must not be empty");
  }
  if (repository_.FindChapter(chapter_id) == nullptr) {
    return Status::NotFound("chapter " + chapter_id + " not found");
  }
  if (count <= 0) {
    questions->clear();
    return Status::Ok();
  }

  int take = std::min(count, kMaxQuestionsPerCall);
  std::vector<const QuestionRecord*> selected =
      repository_.RandomByChapter(chapter_id, take);

  questions->clear();
  questions->reserve(selected.size());
  for (const auto* record : selected) {
    v1::Question question;
    Status status = LoadQuestion(record->id, &question);
    if (!status.ok()) {
      return status;
    }
    questions->push_back(std::move(question));
  }
  return Status::Ok();
}

}  // namespace qbank
