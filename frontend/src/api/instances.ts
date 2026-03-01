import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { get, post, del } from './client';
import type { MystiProxyInstance, InstanceListResponse } from '../types/api';

// Query keys
const QUERY_KEYS = {
  instances: ['instances'] as const,
  instance: (id: string) => ['instance', id] as const,
};

// Fetch instance list
export function useInstances() {
  return useQuery({
    queryKey: QUERY_KEYS.instances,
    queryFn: () => get<InstanceListResponse>('/instances'),
    refetchInterval: 60000, // Refresh every minute
  });
}

// Fetch single instance
export function useInstance(id: string) {
  return useQuery({
    queryKey: QUERY_KEYS.instance(id),
    queryFn: () => get<MystiProxyInstance>(`/instances/${id}`),
    enabled: !!id,
  });
}

// Push config to instance
export function usePushConfig() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (instanceId: string) => post<void>(`/instances/${instanceId}/push`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.instances });
    },
  });
}

// Push config to all instances
export function usePushConfigAll() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => post<void>('/instances/push-all'),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.instances });
    },
  });
}

// Unregister instance
export function useUnregisterInstance() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (instanceId: string) => del<void>(`/instances/${instanceId}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.instances });
    },
  });
}
