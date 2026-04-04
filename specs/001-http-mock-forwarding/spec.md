# Feature Specification: Enhanced HTTP Forwarding and Mock Capabilities

**Feature Branch**: `001-http-mock-forwarding`  
**Created**: 2026-02-28  
**Status**: Draft  
**Input**: User description: "增强一下http转发的能力和http mock的能力,参考一系列的提供http mock的商业产品"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Dynamic Response Templating (Priority: P1)

As a developer or tester, I need to create dynamic mock responses that can vary based on request parameters, so that I can simulate realistic API behaviors without creating multiple static mock configurations.

**Why this priority**: Dynamic response templating is the most critical feature that differentiates modern mock servers from basic static mocks. It enables users to create flexible, maintainable mock configurations that can handle complex scenarios with minimal setup. This is a core feature in WireMock, MockServer, and other commercial products.

**Independent Test**: Can be fully tested by configuring a single mock endpoint with template variables (e.g., `{{request.path.[1]}}`) and verifying that different request paths generate different responses without creating separate mock configurations.

**Acceptance Scenarios**:

1. **Given** a mock configuration with path parameter template `{"id": "{{request.path.[1]}}", "name": "User {{request.path.[1]}}"}`, **When** a request is sent to `/api/users/123`, **Then** the response contains `{"id": "123", "name": "User 123"}`

2. **Given** a mock configuration with query parameter template `{"search": "{{request.query.term}}"}`, **When** a request is sent to `/api/search?term=wiremock`, **Then** the response contains `{"search": "wiremock"}`

3. **Given** a mock configuration with request body template `{"echo": "{{request.body.message}}"}`, **When** a POST request with body `{"message": "hello"}` is sent, **Then** the response contains `{"echo": "hello"}`

4. **Given** a mock configuration with header template `{"auth": "{{request.headers.Authorization}}"}`, **When** a request with `Authorization: Bearer token123` header is sent, **Then** the response contains `{"auth": "Bearer token123"}`

---

### User Story 2 - Request Recording and Playback (Priority: P1)

As a developer, I need to record real HTTP interactions with external services and replay them as mocks, so that I can quickly create realistic mock configurations without manually writing all the response data.

**Why this priority**: Recording and playback is a powerful feature that dramatically reduces the time and effort required to create comprehensive mock configurations. It allows teams to capture real API behavior and use it for testing, development, and documentation. This feature is available in both WireMock and MockServer as a core capability.

**Independent Test**: Can be fully tested by starting recording mode, making requests to a real service, stopping recording, and verifying that subsequent requests to the same endpoints return the recorded responses without contacting the real service.

**Acceptance Scenarios**:

1. **Given** the system is in recording mode targeting `https://api.example.com`, **When** a request to `/users/1` is made and recording is stopped, **Then** a mock configuration is automatically created that returns the recorded response for `/users/1`

2. **Given** recorded interactions exist, **When** a user requests to export the recordings, **Then** the system provides the mock configurations in YAML format that can be version-controlled

3. **Given** recording mode is active, **When** multiple requests to the same endpoint with different parameters are made, **Then** all variations are recorded with their specific request conditions

4. **Given** recorded mock configurations, **When** a request matches a recorded interaction, **Then** the response is returned from the mock without contacting the target service

---

### User Story 3 - Stateful Mock Behavior (Priority: P2)

As a tester, I need to create mock endpoints that maintain state across multiple requests, so that I can simulate complex workflows like login sessions, shopping carts, or multi-step processes.

**Why this priority**: Stateful behavior is essential for testing complex business workflows that involve multiple API calls in sequence. While not all mock scenarios require state, those that do are critical for realistic integration testing. This feature is supported by WireMock through scenarios and MockServer through state management.

**Independent Test**: Can be fully tested by creating a scenario with multiple states (e.g., "Started", "LoggedIn", "Processed") and verifying that requests transition the mock through different states and return different responses based on the current state.

**Acceptance Scenarios**:

1. **Given** a scenario "User Session" with states "Guest", "Authenticated", **When** a POST request to `/login` is sent, **Then** the scenario state transitions to "Authenticated" and subsequent requests return user-specific data

2. **Given** a scenario "Shopping Cart" with initial state "Empty", **When** items are added via POST `/cart/add`, **Then** the cart state updates and GET `/cart` returns the current items

3. **Given** a scenario in state "Processed", **When** a request to `/reset` is sent, **Then** the scenario state returns to initial state

4. **Given** multiple independent scenarios, **When** requests interact with different scenarios, **Then** each scenario maintains its own independent state

---

### User Story 4 - Advanced Request Matching (Priority: P2)

As a developer, I need to match requests based on complex conditions beyond simple URL patterns, so that I can create precise mock behaviors for specific request scenarios.

**Why this priority**: Advanced matching enables users to create highly specific mock configurations that respond differently based on request headers, body content, query parameters, or combinations thereof. This is essential for testing edge cases and error handling. Both WireMock and MockServer provide sophisticated matching capabilities.

**Independent Test**: Can be fully tested by creating mock configurations with various matching criteria (headers, body JSONPath, query parameters, methods) and verifying that only requests matching all criteria receive the mock response.

**Acceptance Scenarios**:

1. **Given** a mock with condition `header: Authorization=Bearer token123`, **When** a request with `Authorization: Bearer token123` is sent, **Then** the mock response is returned; **When** a request with different authorization is sent, **Then** the request is forwarded or returns 404

2. **Given** a mock with condition `body: $.user=admin`, **When** a POST request with body `{"user": "admin"}` is sent, **Then** the mock response is returned

3. **Given** a mock with multiple conditions (path, header, body), **When** a request matches all conditions, **Then** the mock response is returned; **When** any condition fails, **Then** the request is not matched

4. **Given** a mock with regex pattern `path: regex:/api/users/\d+`, **When** a request to `/api/users/123` is sent, **Then** the mock matches; **When** a request to `/api/users/abc` is sent, **Then** the mock does not match

---

### User Story 5 - Proxy with Selective Mocking (Priority: P2)

As a developer, I need to proxy most requests to a real service while mocking specific endpoints, so that I can isolate and test specific functionality without setting up complete mock environments.

**Why this priority**: Selective mocking combined with proxying is a powerful pattern that allows teams to test integrations with real services while mocking only the parts that are unreliable, expensive, or not yet implemented. This hybrid approach is a key feature in MockServer and WireMock.

**Independent Test**: Can be fully tested by configuring the system to proxy to a real service while defining mock responses for specific paths, then verifying that mocked paths return mock responses and other paths are proxied to the real service.

**Acceptance Scenarios**:

1. **Given** the system is configured to proxy to `https://api.example.com` with a mock for `/api/test`, **When** a request to `/api/test` is sent, **Then** the mock response is returned; **When** a request to `/api/real` is sent, **Then** the request is proxied to `https://api.example.com/api/real`

2. **Given** a mock with condition for specific request header, **When** a request matches the condition, **Then** the mock response is returned; **When** a request does not match, **Then** the request is proxied

3. **Given** the proxy target is unavailable, **When** a request that would normally be proxied is sent, **Then** an appropriate error response is returned

4. **Given** both mock and proxy configurations exist for the same path, **When** a request matches the mock conditions, **Then** the mock takes precedence over proxy

---

### User Story 6 - Response Delay and Fault Injection (Priority: P3)

As a tester, I need to simulate network delays, timeouts, and error responses, so that I can test how my application handles adverse conditions and edge cases.

**Why this priority**: Testing resilience and error handling is critical for production-ready applications. The ability to inject delays and faults helps teams verify timeout handling, retry logic, and error recovery. This feature is available in WireMock and MockServer but is secondary to core mocking functionality.

**Independent Test**: Can be fully tested by configuring mocks with various delays and fault conditions, then verifying that the system introduces the specified delays and returns the configured error responses.

**Acceptance Scenarios**:

1. **Given** a mock with `delay_ms: 5000`, **When** a request is sent, **Then** the response is delayed by approximately 5 seconds

2. **Given** a mock configured to return HTTP 500 error, **When** a request is sent, **Then** the response has status 500 with appropriate error body

3. **Given** a mock configured to close connection immediately, **When** a request is sent, **Then** the connection is closed without sending a response

4. **Given** a mock with random delay between 1000-3000ms, **When** multiple requests are sent, **Then** responses have varying delays within the specified range

---

### User Story 7 - Mock Management API (Priority: P3)

As a developer, I need to manage mock configurations through a REST API, so that I can dynamically create, update, and delete mocks during test execution without restarting the service.

**Why this priority**: Dynamic mock management is essential for automated testing scenarios where mocks need to be created or modified during test execution. While important, it's secondary to the core mocking functionality and can be added after the basic features are stable. Both WireMock and MockServer provide admin APIs.

**Independent Test**: Can be fully tested by using the REST API to create, retrieve, update, and delete mock configurations, then verifying that the changes take effect immediately without service restart.

**Acceptance Scenarios**:

1. **Given** the management API is available, **When** a POST request to `/__admin/mocks` with mock configuration is sent, **Then** the mock is created and immediately available for matching

2. **Given** existing mock configurations, **When** a GET request to `/__admin/mocks` is sent, **Then** all active mock configurations are returned

3. **Given** an existing mock, **When** a DELETE request to `/__admin/mocks/{id}` is sent, **Then** the mock is removed and no longer matches requests

4. **Given** existing mocks, **When** a DELETE request to `/__admin/mocks` is sent, **Then** all mocks are cleared

---

### User Story 8 - Request Verification and History (Priority: P3)

As a tester, I need to verify that specific requests were made to mock endpoints, so that I can assert that my application is making the expected API calls during test execution.

**Why this priority**: Request verification is important for behavioral testing where the focus is on verifying that certain API calls were made with correct parameters. This complements response mocking and is particularly useful for testing side effects. WireMock and MockServer both provide verification features.

**Independent Test**: Can be fully tested by making requests to mock endpoints, then querying the verification API to check if specific requests were received, including verification of request count, order, and parameters.

**Acceptance Scenarios**:

1. **Given** requests have been made to mock endpoints, **When** a verification query for a specific request pattern is made, **Then** the system returns whether the request was received and how many times

2. **Given** multiple requests to different endpoints, **When** a verification query for request order is made, **Then** the system returns the sequence of requests in order

3. **Given** a request with specific body content was made, **When** a verification query with body matching condition is made, **Then** the system confirms the request with matching body was received

4. **Given** the request history is enabled, **When** a query for request history is made, **Then** the system returns detailed logs of all received requests including headers, body, and timestamps

---

### Edge Cases

- What happens when a template variable references a non-existent request parameter?
  - System should return an empty string or a configurable default value for missing variables
  
- How does the system handle circular references in stateful scenarios?
  - System should detect and prevent infinite loops in scenario transitions, with a maximum transition limit
  
- What happens when recording mode is active but the target service is unavailable?
  - System should log the error and continue recording other successful requests, or fail gracefully with clear error message
  
- How does the system handle concurrent requests to stateful mocks?
  - System should ensure thread-safe state management to prevent race conditions
  
- What happens when mock configuration files are malformed?
  - System should validate configuration on load and provide clear error messages indicating the issue
  
- How does the system handle very large request/response bodies during recording?
  - System should have configurable limits on body size to prevent memory issues
  
- What happens when the same request matches multiple mock configurations?
  - System should use priority/ordering rules to select the most specific or first matching mock

## Requirements *(mandatory)*

### Functional Requirements

**Dynamic Response Templating**

- **FR-001**: System MUST support template variables in mock response bodies that reference request path parameters
- **FR-002**: System MUST support template variables that reference request query parameters
- **FR-003**: System MUST support template variables that reference request headers
- **FR-004**: System MUST support template variables that reference request body fields using JSONPath
- **FR-005**: System MUST support template syntax compatible with industry-standard templating engines (e.g., Handlebars-style `{{variable}}`)
- **FR-006**: System MUST provide default values for missing template variables to prevent errors

**Request Recording and Playback**

- **FR-007**: System MUST allow users to start recording mode targeting a specific upstream service
- **FR-008**: System MUST automatically create mock configurations from recorded request/response pairs
- **FR-009**: System MUST capture request method, path, query parameters, headers, and body during recording
- **FR-010**: System MUST capture response status, headers, and body during recording
- **FR-011**: System MUST allow users to stop recording and export recorded mocks as configuration files
- **FR-012**: System MUST support filtering which requests to record based on path patterns or other criteria

**Stateful Mock Behavior**

- **FR-013**: System MUST support scenario-based state management for mocks
- **FR-014**: System MUST allow mocks to transition between named states based on requests
- **FR-015**: System MUST allow different responses for the same request based on current scenario state
- **FR-016**: System MUST support resetting scenarios to initial state
- **FR-017**: System MUST support multiple independent scenarios running concurrently

**Advanced Request Matching**

- **FR-018**: System MUST support matching requests by HTTP method (GET, POST, PUT, DELETE, etc.)
- **FR-019**: System MUST support matching requests by exact path, prefix path, and regex pattern
- **FR-020**: System MUST support matching requests by query parameter presence and values
- **FR-021**: System MUST support matching requests by header presence and values
- **FR-022**: System MUST support matching requests by body content using JSONPath expressions
- **FR-023**: System MUST support combining multiple matching conditions with AND logic
- **FR-024**: System MUST support regex patterns in header, query, and body matching

**Proxy with Selective Mocking**

- **FR-025**: System MUST support proxying unmatched requests to a configured target service
- **FR-026**: System MUST allow mock configurations to take precedence over proxy for matching requests
- **FR-027**: System MUST preserve request headers, body, and method when proxying
- **FR-028**: System MUST handle proxy connection failures gracefully with appropriate error responses
- **FR-029**: System MUST support both HTTP and HTTPS proxying with proper certificate handling

**Response Delay and Fault Injection**

- **FR-030**: System MUST support configuring fixed delays for mock responses
- **FR-031**: System MUST support configuring random delays within a specified range
- **FR-032**: System MUST support configuring error status codes (4xx, 5xx) for mock responses
- **FR-033**: System MUST support configuring connection termination without response
- **FR-034**: System MUST support configuring malformed responses for testing error handling

**Mock Management API**

- **FR-035**: System MUST provide a REST API for creating mock configurations
- **FR-036**: System MUST provide a REST API for retrieving current mock configurations
- **FR-037**: System MUST provide a REST API for updating existing mock configurations
- **FR-038**: System MUST provide a REST API for deleting mock configurations
- **FR-039**: System MUST apply mock configuration changes immediately without service restart
- **FR-040**: System MUST support bulk operations for managing multiple mocks

**Request Verification and History**

- **FR-041**: System MUST maintain a history of received requests for verification
- **FR-042**: System MUST provide an API to verify if a specific request pattern was received
- **FR-043**: System MUST support verifying request count for a specific pattern
- **FR-044**: System MUST support verifying request sequence/order
- **FR-045**: System MUST allow clearing request history
- **FR-046**: System MUST support configurable request history retention limits

**Configuration and Usability**

- **FR-047**: System MUST support YAML configuration format for mock definitions
- **FR-048**: System MUST validate mock configurations and provide clear error messages
- **FR-049**: System MUST support hot-reloading of configuration files
- **FR-050**: System MUST provide detailed logging for debugging mock matching and responses

### Key Entities

- **Mock Configuration**: Defines a mock endpoint including matching criteria (path, method, headers, body), response template, state transitions, and metadata. Each configuration represents a single mock behavior rule.

- **Scenario**: Represents a stateful workflow with named states. Contains state transition rules and tracks current state. Multiple scenarios can exist independently.

- **Recording Session**: Represents an active recording session targeting a specific upstream service. Captures request/response pairs and converts them to mock configurations.

- **Request History Entry**: Represents a single received request with full details (method, path, headers, body, timestamp). Used for verification and debugging.

- **Template Context**: Contains all available variables for template resolution including request parameters, headers, body, and scenario state.

## Success Criteria *(mandatory)*

### Measurable Outcomes

**Performance and Reliability**

- **SC-001**: Mock response time must be under 10 milliseconds for static responses without templating
- **SC-002**: Mock response time must be under 50 milliseconds for responses with template resolution
- **SC-003**: System must handle at least 10,000 concurrent connections without degradation
- **SC-004**: Memory usage must remain stable during extended recording sessions (no memory leaks)

**Usability and Developer Experience**

- **SC-005**: Users can create a basic mock endpoint in under 2 minutes using YAML configuration
- **SC-006**: Users can create dynamic mocks using templates without consulting documentation for common use cases
- **SC-007**: Users can set up recording and generate mock configurations in under 5 minutes
- **SC-008**: Error messages for invalid configurations clearly identify the issue and suggest corrections

**Feature Completeness**

- **SC-009**: Template system supports at least 90% of common dynamic response use cases identified in WireMock and MockServer documentation
- **SC-010**: Request matching system supports all standard HTTP matching criteria (method, path, headers, query, body)
- **SC-011**: Recording feature captures all essential request and response data for stateless APIs
- **SC-012**: Stateful mock scenarios support at least 10 states per scenario and 20 concurrent scenarios

**Testing and Quality**

- **SC-013**: All core features have automated tests with at least 80% code coverage
- **SC-014**: System passes all existing integration tests without regression
- **SC-015**: Mock configurations from WireMock can be migrated to MystiProxy format with minimal manual conversion

**Operational**

- **SC-016**: Configuration hot-reload completes within 1 second without dropping in-flight requests
- **SC-017**: Management API responds to all requests within 100 milliseconds under normal load
- **SC-018**: Request history query returns results within 500 milliseconds for up to 10,000 stored requests

## Assumptions

- Users have basic familiarity with YAML configuration format
- Users understand HTTP protocol basics (methods, headers, status codes)
- Template syntax follows Handlebars-style conventions which are widely known
- Recording feature requires network connectivity to target services
- Stateful scenarios are used for testing workflows, not for production traffic management
- Mock configurations are primarily managed through configuration files, with API management as a secondary interface
- Default template variable values (empty string for missing variables) are acceptable for most use cases
- Request history has configurable retention to manage memory usage in long-running instances
