import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { get, post, put, del } from './client';
import type {
  Environment,
  EnvironmentCreateRequest,
  EnvironmentUpdateRequest,
  EnvironmentListResponse,
} from '../types/api';

// Query keys
const QUERY_KEYS = {
  environments: ['environments'] as const,
  environment: (id: string) => ['environment', id] as const,
};

// Fetch environment list
export function useEnvironments() {
  return useQuery({
    queryKey: QUERY_KEYS.environments,
    queryFn: () => get<EnvironmentListResponse>('/environments'),
  });
}

// Fetch single environment
export function useEnvironment(id: string) {
  return useQuery({
    queryKey: QUERY_KEYS.environment(id),
    queryFn: () => get<Environment>(`/environments/${id}`),
    enabled: !!id,
  });
}

// Create environment
export function useCreateEnvironment() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: EnvironmentCreateRequest) => post<Environment>('/environments', data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.environments });
    },
  });
}

// Update environment
export function useUpdateEnvironment(id: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: EnvironmentUpdateRequest) => put<Environment>(`/environments/${id}`, data),
    onSuccess: (updatedData) => {
      queryClient.setQueryData(QUERY_KEYS.environment(id), updatedData);
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.environments });
    },
  });
}

// Delete environment
export function useDeleteEnvironment() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => del<void>(`/environments/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.environments });
    },
  });
}
