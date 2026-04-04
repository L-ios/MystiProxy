import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { get, put, del } from './client';
import type { ConflictResponse, ConflictListResponse, ConflictResolveRequest } from '../types/api';

// Query keys
const QUERY_KEYS = {
  conflicts: ['conflicts'] as const,
  conflict: (id: string) => ['conflict', id] as const,
};

// Fetch conflict list
export function useConflicts() {
  return useQuery({
    queryKey: QUERY_KEYS.conflicts,
    queryFn: () => get<ConflictListResponse>('/conflicts'),
  });
}

// Fetch single conflict
export function useConflict(configId: string) {
  return useQuery({
    queryKey: QUERY_KEYS.conflict(configId),
    queryFn: () => get<ConflictResponse>(`/conflicts/${configId}`),
    enabled: !!configId,
  });
}

// Resolve conflict
export function useResolveConflict() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ configId, data }: { configId: string; data: ConflictResolveRequest }) =>
      put<void>(`/conflicts/${configId}/resolve`, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.conflicts });
      queryClient.invalidateQueries({ queryKey: ['mocks'] });
    },
  });
}

// Dismiss conflict (keep both versions)
export function useDismissConflict() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (configId: string) => del<void>(`/conflicts/${configId}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.conflicts });
    },
  });
}
