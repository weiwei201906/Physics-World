import { gqlRequest } from './graphql'
import type {
  ConfigEntry,
  ConfigMutationPayload,
  ConfigSchemaEntry,
  ConfigAuditEntry,
  QuestionVersion
} from '@/types'

export interface QuestionUploadInput {
  chapterId: string
  difficulty: number
  content: {
    stem: string
    imageUrl: string | null
    explanation: string | null
    options: Array<{ id: string; label: string; text: string }>
    correctOptionId: string
  }
}

const CONFIG_FIELDS = 'namespace key value updatedAt'
const MUTATION_FIELDS = 'applied degraded entry { namespace key value updatedAt } errorMessage'
const SCHEMA_FIELDS = 'namespace key type required defaultValue description'
const AUDIT_FIELDS = 'id namespace key oldValue newValue changedBy changedAt'
const VERSION_FIELDS = 'id questionId version deletedAt createdAt'

const CONFIGS_QUERY = `
  query AdminConfigs($namespace: String!) {
    adminConfigs(namespace: $namespace) { ${CONFIG_FIELDS} }
  }
`

const CONFIG_SCHEMAS_QUERY = `
  query AdminConfigSchemas($namespace: String) {
    adminConfigSchemas(namespace: $namespace) { ${SCHEMA_FIELDS} }
  }
`

const CONFIG_HISTORY_QUERY = `
  query AdminConfigHistory($namespace: String!, $key: String!, $limit: Int!) {
    adminConfigHistory(namespace: $namespace, key: $key, limit: $limit) { ${AUDIT_FIELDS} }
  }
`

const QUESTION_VERSIONS_QUERY = `
  query AdminQuestionVersions($questionId: ID!) {
    adminQuestionVersions(questionId: $questionId) { ${VERSION_FIELDS} }
  }
`

const UPSERT_CONFIG_MUTATION = `
  mutation AdminUpsertConfig($namespace: String!, $key: String!, $value: String!) {
    adminUpsertConfig(namespace: $namespace, key: $key, value: $value) { ${MUTATION_FIELDS} }
  }
`

const DELETE_CONFIG_MUTATION = `
  mutation AdminDeleteConfig($namespace: String!, $key: String!) {
    adminDeleteConfig(namespace: $namespace, key: $key) { ${MUTATION_FIELDS} }
  }
`

const ROLLBACK_CONFIG_MUTATION = `
  mutation AdminRollbackConfig($namespace: String!, $key: String!, $version: Int!) {
    adminRollbackConfig(namespace: $namespace, key: $key, version: $version) { ${MUTATION_FIELDS} }
  }
`

const REGISTER_SCHEMA_MUTATION = `
  mutation AdminRegisterConfigSchema($input: ConfigSchemaInput!) {
    adminRegisterConfigSchema(input: $input) { ${SCHEMA_FIELDS} }
  }
`

const UPLOAD_QUESTION_MUTATION = `
  mutation AdminUploadQuestion($input: QuestionUploadInput!) {
    adminUploadQuestion(input: $input) { ${VERSION_FIELDS} }
  }
`

const UPDATE_QUESTION_MUTATION = `
  mutation AdminUpdateQuestion($questionId: ID!, $input: QuestionUploadInput!) {
    adminUpdateQuestion(questionId: $questionId, input: $input) { ${VERSION_FIELDS} }
  }
`

const SOFT_DELETE_QUESTION_MUTATION = `
  mutation AdminSoftDeleteQuestion($questionId: ID!) {
    adminSoftDeleteQuestion(questionId: $questionId)
  }
`

export async function fetchAdminConfigs(namespace: string): Promise<ConfigEntry[]> {
  return gqlRequest<{ adminConfigs: ConfigEntry[] }>(CONFIGS_QUERY, { namespace }).then(
    (d) => d.adminConfigs
  )
}

export async function fetchAdminConfigSchemas(
  namespace?: string
): Promise<ConfigSchemaEntry[]> {
  return gqlRequest<{ adminConfigSchemas: ConfigSchemaEntry[] }>(CONFIG_SCHEMAS_QUERY, {
    namespace: namespace ?? null
  }).then((d) => d.adminConfigSchemas)
}

export async function fetchAdminConfigHistory(
  namespace: string,
  key: string,
  limit: number
): Promise<ConfigAuditEntry[]> {
  return gqlRequest<{ adminConfigHistory: ConfigAuditEntry[] }>(CONFIG_HISTORY_QUERY, {
    namespace,
    key,
    limit
  }).then((d) => d.adminConfigHistory)
}

export async function fetchQuestionVersions(questionId: string): Promise<QuestionVersion[]> {
  return gqlRequest<{ adminQuestionVersions: QuestionVersion[] }>(QUESTION_VERSIONS_QUERY, {
    questionId
  }).then((d) => d.adminQuestionVersions)
}

export async function upsertConfig(
  namespace: string,
  key: string,
  value: string
): Promise<ConfigMutationPayload> {
  return gqlRequest<{ adminUpsertConfig: ConfigMutationPayload }>(UPSERT_CONFIG_MUTATION, {
    namespace,
    key,
    value
  }).then((d) => d.adminUpsertConfig)
}

export async function deleteConfig(
  namespace: string,
  key: string
): Promise<ConfigMutationPayload> {
  return gqlRequest<{ adminDeleteConfig: ConfigMutationPayload }>(DELETE_CONFIG_MUTATION, {
    namespace,
    key
  }).then((d) => d.adminDeleteConfig)
}

export async function rollbackConfig(
  namespace: string,
  key: string,
  version: number
): Promise<ConfigMutationPayload> {
  return gqlRequest<{ adminRollbackConfig: ConfigMutationPayload }>(ROLLBACK_CONFIG_MUTATION, {
    namespace,
    key,
    version
  }).then((d) => d.adminRollbackConfig)
}

export async function registerConfigSchema(input: {
  namespace: string
  key: string
  type: string
  required: boolean
  defaultValue?: string | null
  description?: string | null
}): Promise<ConfigSchemaEntry> {
  return gqlRequest<{ adminRegisterConfigSchema: ConfigSchemaEntry }>(
    REGISTER_SCHEMA_MUTATION,
    { input }
  ).then((d) => d.adminRegisterConfigSchema)
}

export async function uploadQuestion(input: QuestionUploadInput): Promise<QuestionVersion> {
  return gqlRequest<{ adminUploadQuestion: QuestionVersion }>(UPLOAD_QUESTION_MUTATION, {
    input
  }).then((d) => d.adminUploadQuestion)
}

export async function updateQuestion(
  questionId: string,
  input: QuestionUploadInput
): Promise<QuestionVersion> {
  return gqlRequest<{ adminUpdateQuestion: QuestionVersion }>(UPDATE_QUESTION_MUTATION, {
    questionId,
    input
  }).then((d) => d.adminUpdateQuestion)
}

export async function softDeleteQuestion(questionId: string): Promise<boolean> {
  return gqlRequest<{ adminSoftDeleteQuestion: boolean }>(SOFT_DELETE_QUESTION_MUTATION, {
    questionId
  }).then((d) => d.adminSoftDeleteQuestion)
}
