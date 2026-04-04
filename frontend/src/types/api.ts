// API Types based on OpenAPI specification

export interface MockConfiguration {
  id: string;
  name: string;
  path: string;
  method: string;
  team_id?: string;
  environment_id?: string;
  matching_rules: MatchingRules;
  response_config: ResponseConfig;
  state_config?: StateConfig;
  source: 'central' | 'local';
  version_vector: Record<string, number>;
  content_hash: string;
  created_at: string;
  updated_at: string;
  created_by?: string;
}

export interface MatchingRules {
  path_pattern?: string;
  path_pattern_type?: 'exact' | 'prefix' | 'regex';
  headers?: HeaderMatch[];
  query_params?: QueryParamMatch[];
  body?: BodyMatch;
}

export interface HeaderMatch {
  name: string;
  value: string;
  match_type?: 'exact' | 'regex' | 'exists';
}

export interface QueryParamMatch {
  name: string;
  value: string;
  match_type?: 'exact' | 'regex' | 'exists';
}

export interface BodyMatch {
  json_path?: string;
  value?: string;
  match_type?: 'exact' | 'regex' | 'json_path';
}

export interface ResponseConfig {
  status: number;
  headers?: Record<string, string>;
  body?: ResponseBody;
  delay_ms?: number;
}

export interface ResponseBody {
  type: 'static' | 'template' | 'file' | 'script';
  content?: string;
  template_vars?: TemplateVar[];
}

export interface TemplateVar {
  name: string;
  source: 'path' | 'query' | 'header' | 'body';
  path?: string;
}

export interface StateConfig {
  initial_state?: string;
  transitions?: StateTransition[];
}

export interface StateTransition {
  from_state: string;
  to_state: string;
  trigger?: StateTrigger;
  response?: ResponseConfig;
}

export interface StateTrigger {
  type?: 'request' | 'body' | 'header';
  condition?: string;
}

export interface MockCreateRequest {
  name: string;
  path: string;
  method: string;
  team_id?: string;
  environment_id?: string;
  matching_rules: MatchingRules;
  response_config: ResponseConfig;
  state_config?: StateConfig;
}

export interface MockUpdateRequest {
  name?: string;
  matching_rules?: MatchingRules;
  response_config?: ResponseConfig;
  state_config?: StateConfig;
  version_vector?: Record<string, number>;
}

export interface MockListResponse {
  data: MockConfiguration[];
  pagination: Pagination;
}

export interface Pagination {
  page: number;
  limit: number;
  total: number;
  total_pages: number;
}

export interface MockFilter {
  environment?: string;
  team?: string;
  path?: string;
  method?: string;
  page?: number;
  limit?: number;
}

export interface Environment {
  id: string;
  name: string;
  description?: string;
  endpoints?: Record<string, string>;
  is_template?: boolean;
  template_id?: string;
  created_at: string;
  updated_at: string;
}

export interface EnvironmentCreateRequest {
  name: string;
  description?: string;
  endpoints?: Record<string, string>;
  template_id?: string;
}

export interface EnvironmentUpdateRequest {
  name?: string;
  description?: string;
  endpoints?: Record<string, string>;
}

export interface EnvironmentListResponse {
  data: Environment[];
}

export interface MystiProxyInstance {
  id: string;
  name: string;
  endpoint_url: string;
  sync_status: 'connected' | 'disconnected' | 'syncing' | 'conflict';
  last_sync_at?: string;
  config_checksum?: string;
  registered_at: string;
  last_heartbeat?: string;
}

export interface InstanceListResponse {
  data: MystiProxyInstance[];
}

export interface ApiError {
  code: string;
  message: string;
}

export interface ValidationError extends ApiError {
  details: Array<{
    field: string;
    message: string;
  }>;
}

export interface ConflictResponse {
  config_id: string;
  local_version: MockConfiguration;
  central_version: MockConfiguration;
  detected_at: string;
}

export interface ConflictResolveRequest {
  resolution: 'keep_local' | 'keep_central' | 'merge';
  merged_config?: MockConfiguration;
}

// ============ Sync Types ============
export interface SyncStatus {
  connected: boolean;
  last_sync_at?: string;
  sync_in_progress: boolean;
  pending_changes: number;
  central_url?: string;
}

export interface SyncRequest {
  force?: boolean;
}

export interface SyncResponse {
  success: boolean;
  synced_count: number;
  conflicts: ConflictResponse[];
  synced_at: string;
}

// ============ Conflict Types ============
export interface ConflictListResponse {
  data: ConflictResponse[];
  total: number;
}

// ============ Analytics Types ============
export interface AnalyticsOverview {
  total_requests: number;
  avg_response_time: number;
  error_rate: number;
  active_mocks: number;
}

export interface RequestStats {
  date: string;
  count: number;
  success_count: number;
  error_count: number;
}

export interface ResponseTimeStats {
  date: string;
  avg_time: number;
  p50: number;
  p95: number;
  p99: number;
}

export interface MockUsageStats {
  mock_id: string;
  mock_name: string;
  request_count: number;
  avg_response_time: number;
  error_count: number;
}

export interface AnalyticsFilter {
  start_date?: string;
  end_date?: string;
  mock_id?: string;
  environment?: string;
}

export interface AnalyticsResponse {
  overview: AnalyticsOverview;
  request_stats: RequestStats[];
  response_time_stats: ResponseTimeStats[];
  top_mocks: MockUsageStats[];
}

// ============ User & Auth Types ============
export interface User {
  id: string;
  username: string;
  email: string;
  role: 'admin' | 'user' | 'viewer';
  team_id?: string;
  created_at: string;
  updated_at: string;
  last_login_at?: string;
}

export interface UserCreateRequest {
  username: string;
  email: string;
  password: string;
  role: 'admin' | 'user' | 'viewer';
  team_id?: string;
}

export interface UserUpdateRequest {
  username?: string;
  email?: string;
  role?: 'admin' | 'user' | 'viewer';
  team_id?: string;
}

export interface UserListResponse {
  data: User[];
  total: number;
}

export interface LoginRequest {
  username: string;
  password: string;
}

export interface LoginResponse {
  token: string;
  user: User;
  expires_at: string;
}

export interface ChangePasswordRequest {
  old_password: string;
  new_password: string;
}

// ============ Import/Export Types ============
export interface ExportRequest {
  format: 'json' | 'yaml';
  include_environments?: boolean;
  include_teams?: boolean;
  mock_ids?: string[];
}

export interface ExportResponse {
  data: string;
  filename: string;
  format: 'json' | 'yaml';
}

export interface ImportRequest {
  data: string;
  format: 'json' | 'yaml';
  merge_strategy: 'replace' | 'merge' | 'skip_existing';
}

export interface ImportResponse {
  imported_count: number;
  skipped_count: number;
  error_count: number;
  errors?: Array<{
    path: string;
    message: string;
  }>;
}

// ============ Team Types ============
export interface Team {
  id: string;
  name: string;
  description?: string;
  members: string[];
  created_at: string;
  updated_at: string;
}

export interface TeamCreateRequest {
  name: string;
  description?: string;
  members?: string[];
}

export interface TeamUpdateRequest {
  name?: string;
  description?: string;
  members?: string[];
}

export interface TeamListResponse {
  data: Team[];
  total: number;
}

// ============ Settings Types ============
export interface SystemSettings {
  central_url: string;
  sync_interval: number;
  log_level: 'debug' | 'info' | 'warn' | 'error';
  max_request_history: number;
  default_environment?: string;
}

export interface SettingsUpdateRequest {
  central_url?: string;
  sync_interval?: number;
  log_level?: 'debug' | 'info' | 'warn' | 'error';
  max_request_history?: number;
  default_environment?: string;
}
