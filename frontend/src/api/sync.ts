import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { get, post } from './client';
import type { SyncStatus, SyncRequest, SyncResponse } from '../types/api';

// Query keys
const QUERY_KEYS = {
  syncStatus: ['sync', 'status'] as const,
};

// Fetch sync status
export function useSyncStatus() {
  return useQuery({
    queryKey: QUERY_KEYS.syncStatus,
    queryFn: () => get<SyncStatus>('/sync/status'),
    refetchInterval: 30000, // Refresh every 30 seconds
  });
}

// Trigger sync
export function useSync() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data?: SyncRequest) => post<SyncResponse>('/sync', data),
    onSuccess: () => {
      // Invalidate sync status
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.syncStatus });
      // Invalidate mocks list
      queryClient.invalidateQueries({ queryKey: ['mocks'] });
      // Invalidate conflicts
      queryClient.invalidateQueries({ queryKey: ['conflicts'] });
    },
  });
}
