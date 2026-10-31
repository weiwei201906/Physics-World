#include "backend/question-bank/src/repository.h"

#include <algorithm>
#include <array>
#include <map>
#include <random>
#include <unordered_map>
#include <utility>

namespace qbank {
namespace {

constexpr std::array<const char*, 8> kChapterIds = {
    "mechanics", "thermodynamics", "waves",       "optics",
    "electromagnetism", "modern-physics", "relativity", "quantum",
};

constexpr int kQuestionsPerChapter = 3;
constexpr std::int64_t kBaseUpdatedAtMs = 1700000000000LL;

std::uint64_t Fnv1a(const std::string& value) {
  std::uint64_t hash = 1469598103934665603ULL;
  for (unsigned char c : value) {
    hash ^= c;
    hash *= 1099511628211ULL;
  }
  return hash;
}

}  // namespace

Repository::Repository(std::uint64_t seed) : seed_(seed) {
  for (std::size_t chapter_index = 0; chapter_index < kChapterIds.size();
       ++chapter_index) {
    std::string chapter_id = kChapterIds[chapter_index];
    auto& chapter = chapters_[chapter_id];

    for (int i = 0; i < kQuestionsPerChapter; ++i) {
      QuestionRecord record;
      record.id = chapter_id + "-" + std::to_string(i + 1);
      record.chapter_id = chapter_id;
      record.difficulty = 1 + static_cast<int>(
                                  (chapter_index + static_cast<std::size_t>(i)) % 3);
      record.stem = "Physics question " + record.id + ": " + chapter_id +
                    " concept " + std::to_string(i + 1) + ".";
      record.image_url = "https://cos.example/qbank/" + record.id + ".png";
      record.explanation = "Worked solution for " + record.id + ".";
      record.updated_at_epoch_ms =
          kBaseUpdatedAtMs +
          static_cast<std::int64_t>((chapter_index * kQuestionsPerChapter +
                                     static_cast<std::size_t>(i)) * 60000);

      for (int option_index = 0; option_index < 4; ++option_index) {
        QuestionOptionRecord option;
        option.id = std::string(1, static_cast<char>('a' + option_index));
        option.label = std::string(1, static_cast<char>('A' + option_index));
        option.text = "Option " + option.label + " for " + record.id;
        record.options.push_back(std::move(option));
      }
      // Answer keys rotate a,b,c,d over the seeded question sequence.
      record.answer_key = static_cast<char>(
          'a' + (chapter_index + static_cast<std::size_t>(i)) % 4);

      chapter.push_back(std::move(record));
    }
  }

  for (auto& [chapter_id, chapter] : chapters_) {
    for (auto& record : chapter) {
      by_id_[record.id] = &record;
    }
  }
}

const QuestionRecord* Repository::Find(const std::string& question_id) const {
  auto iter = by_id_.find(question_id);
  return iter == by_id_.end() ? nullptr : iter->second;
}

const std::vector<QuestionRecord>* Repository::FindChapter(
    const std::string& chapter_id) const {
  auto iter = chapters_.find(chapter_id);
  return iter == chapters_.end() ? nullptr : &iter->second;
}

std::vector<const QuestionRecord*> Repository::RandomByChapter(
    const std::string& chapter_id, int count) const {
  auto chapter = FindChapter(chapter_id);
  if (chapter == nullptr || count <= 0) {
    return {};
  }

  std::vector<const QuestionRecord*> candidates;
  candidates.reserve(chapter->size());
  for (const auto& record : *chapter) {
    candidates.push_back(&record);
  }

  std::mt19937_64 rng(seed_ ^ Fnv1a(chapter_id));
  std::shuffle(candidates.begin(), candidates.end(), rng);

  std::size_t take = std::min(candidates.size(),
                              static_cast<std::size_t>(count));
  candidates.resize(take);
  return candidates;
}

}  // namespace qbank
