import { useQuery } from '@tanstack/react-query';
import { get } from './client';
import type { AnalyticsResponse, AnalyticsFilter } from '../types/api';

// Query keys
const QUERY_KEYS = {
  analytics: (filter?: AnalyticsFilter) => ['analytics', filter] as const,
};

// Fetch analytics data
export function useAnalytics(filter?: AnalyticsFilter) {
  return useQuery({
    queryKey: QUERY_KEYS.analytics(filter),
    queryFn: () => {
      const params = new URLSearchParams();
      if (filter?.start_date) params.append('start_date', filter.start_date);
      if (filter?.end_date) params.append('end_date', filter.end_date);
      if (filter?.mock_id) params.append('mock_id', filter.mock_id);
      if (filter?.environment) params.append('environment', filter.environment);

      const queryString = params.toString();
      return get<AnalyticsResponse>(`/analytics${queryString ? `?${queryString}` : ''}`);
    },
    staleTime: 5 * 60 * 1000, // 5 minutes
  });
}
