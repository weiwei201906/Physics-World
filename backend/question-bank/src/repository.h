#ifndef QBANK_BACKEND_QUESTION_BANK_SRC_REPOSITORY_H_
#define QBANK_BACKEND_QUESTION_BANK_SRC_REPOSITORY_H_

#include <cstdint>
#include <map>
#include <string>
#include <unordered_map>
#include <vector>

namespace qbank {

struct QuestionOptionRecord {
  std::string id;
  std::string label;
  std::string text;
};

struct QuestionRecord {
  std::string id;
  std::string chapter_id;
  int difficulty = 1;
  std::string stem;
  std::string image_url;
  std::string explanation;
  std::vector<QuestionOptionRecord> options;  // Always a-d.
  char answer_key = 'a';                      // Rotates per seeded question.
  std::int64_t updated_at_epoch_ms = 0;
};

// In-process seed data: 8 chapters with 3 questions each.  This stands in for
// the PG / COS authoritative store until the admin write path exists.
class Repository {
 public:
  explicit Repository(std::uint64_t seed);

  const QuestionRecord* Find(const std::string& question_id) const;

  // Returns nullptr when the chapter is unknown.
  const std::vector<QuestionRecord>* FindChapter(
      const std::string& chapter_id) const;

  // Deterministic for a given (seed, chapter_id): the same call always yields
  // the same prefix ordering.
  std::vector<const QuestionRecord*> RandomByChapter(
      const std::string& chapter_id, int count) const;

  std::uint64_t seed() const { return seed_; }

  // Read-only view used to seed the COS stand-in with the authoritative
  // objects.  Keys are chapter ids, values are the chapter's questions.
  const std::map<std::string, std::vector<QuestionRecord>>& chapters() const {
    return chapters_;
  }

 private:
  std::uint64_t seed_;
  std::map<std::string, std::vector<QuestionRecord>> chapters_;
  std::unordered_map<std::string, const QuestionRecord*> by_id_;
};

}  // namespace qbank

#endif  // QBANK_BACKEND_QUESTION_BANK_SRC_REPOSITORY_H_
