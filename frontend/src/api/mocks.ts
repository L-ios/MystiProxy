import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { get, post, put, del } from './client';
import type {
  MockConfiguration,
  MockCreateRequest,
  MockUpdateRequest,
  MockListResponse,
  MockFilter,
} from '../types/api';

// Query keys
const QUERY_KEYS = {
  mocks: (filter?: MockFilter) => ['mocks', filter] as const,
  mock: (id: string) => ['mock', id] as const,
};

// Fetch mock list
export function useMocks(filter?: MockFilter) {
  return useQuery({
    queryKey: QUERY_KEYS.mocks(filter),
    queryFn: () => {
      const params = new URLSearchParams();
      if (filter?.environment) params.append('environment', filter.environment);
      if (filter?.team) params.append('team', filter.team);
      if (filter?.path) params.append('path', filter.path);
      if (filter?.method) params.append('method', filter.method);
      if (filter?.page) params.append('page', filter.page.toString());
      if (filter?.limit) params.append('limit', filter.limit.toString());

      const queryString = params.toString();
      return get<MockListResponse>(`/mocks${queryString ? `?${queryString}` : ''}`);
    },
  });
}

// Fetch single mock
export function useMock(id: string) {
  return useQuery({
    queryKey: QUERY_KEYS.mock(id),
    queryFn: () => get<MockConfiguration>(`/mocks/${id}`),
    enabled: !!id,
  });
}

// Create mock
export function useCreateMock() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: MockCreateRequest) => post<MockConfiguration>('/mocks', data),
    onSuccess: () => {
      // Invalidate and refetch mocks list
      queryClient.invalidateQueries({ queryKey: ['mocks'] });
    },
  });
}

// Update mock
export function useUpdateMock(id: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: MockUpdateRequest) => put<MockConfiguration>(`/mocks/${id}`, data),
    onSuccess: (updatedMock) => {
      // Update the specific mock in cache
      queryClient.setQueryData(QUERY_KEYS.mock(id), updatedMock);
      // Invalidate mocks list
      queryClient.invalidateQueries({ queryKey: ['mocks'] });
    },
  });
}

// Delete mock
export function useDeleteMock() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => del<void>(`/mocks/${id}`),
    onSuccess: () => {
      // Invalidate mocks list
      queryClient.invalidateQueries({ queryKey: ['mocks'] });
    },
  });
}

// Batch delete mocks
export function useBatchDeleteMocks() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (ids: string[]) => {
      await Promise.all(ids.map((id) => del<void>(`/mocks/${id}`)));
    },
    onSuccess: () => {
      // Invalidate mocks list
      queryClient.invalidateQueries({ queryKey: ['mocks'] });
    },
  });
}
