import http from './http'

export interface GraphQLError {
  message: string
  locations?: Array<{ line: number; column: number }>
  path?: Array<string | number>
}

export interface GraphQLResponse<T> {
  data?: T
  errors?: GraphQLError[]
}

const GRAPHQL_ENDPOINT = import.meta.env.VITE_GRAPHQL_ENDPOINT ?? '/graphql'
const REC_GRAPHQL_ENDPOINT = import.meta.env.VITE_REC_GRAPHQL_ENDPOINT ?? '/rec/graphql'

/**
 * Execute a GraphQL operation against user-api (/graphql).
 * Business-level errors (HTTP 200 + errors) do NOT trigger token refresh;
 * only HTTP 401 from the gateway does.
 */
export async function gqlRequest<T>(
  query: string,
  variables?: Record<string, unknown>,
  signal?: AbortSignal
): Promise<T> {
  const response = await http.post<GraphQLResponse<T>>(
    GRAPHQL_ENDPOINT,
    { query, variables },
    {
      headers: { 'Content-Type': 'application/json' },
      signal
    }
  )

  if (response.data.errors && response.data.errors.length > 0) {
    throw new GraphQLOperationError(response.data.errors)
  }
  return response.data.data as T
}

/**
 * Execute a GraphQL operation against recommendation-service (/rec/graphql).
 */
export async function recGqlRequest<T>(
  query: string,
  variables?: Record<string, unknown>,
  signal?: AbortSignal
): Promise<T> {
  const response = await http.post<GraphQLResponse<T>>(
    REC_GRAPHQL_ENDPOINT,
    { query, variables },
    {
      headers: { 'Content-Type': 'application/json' },
      signal
    }
  )

  if (response.data.errors && response.data.errors.length > 0) {
    throw new GraphQLOperationError(response.data.errors)
  }
  return response.data.data as T
}

export class GraphQLOperationError extends Error {
  public readonly errors: GraphQLError[]

  constructor(errors: GraphQLError[]) {
    super(errors.map((e) => e.message).join('; '))
    this.name = 'GraphQLOperationError'
    this.errors = errors
  }
}
