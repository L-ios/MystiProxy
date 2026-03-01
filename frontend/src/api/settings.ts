import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { get, put } from './client';
import type { SystemSettings, SettingsUpdateRequest } from '../types/api';

// Query keys
const QUERY_KEYS = {
  settings: ['settings'] as const,
};

// Fetch system settings
export function useSettings() {
  return useQuery({
    queryKey: QUERY_KEYS.settings,
    queryFn: () => get<SystemSettings>('/settings'),
  });
}

// Update system settings
export function useUpdateSettings() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: SettingsUpdateRequest) => put<SystemSettings>('/settings', data),
    onSuccess: (updatedSettings) => {
      queryClient.setQueryData(QUERY_KEYS.settings, updatedSettings);
    },
  });
}
