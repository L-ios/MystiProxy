import axios, { AxiosError } from 'axios';
import type { AxiosInstance, AxiosRequestConfig } from 'axios';
import type { ApiError, ValidationError } from '../types/api';

// API base URL - can be configured via environment variable
const API_BASE_URL = import.meta.env.VITE_API_BASE_URL || 'http://localhost:8080/api/v1';

// Create axios instance with default config
const createApiClient = (): AxiosInstance => {
  const client = axios.create({
    baseURL: API_BASE_URL,
    timeout: 10000,
    headers: {
      'Content-Type': 'application/json',
    },
  });

  // Request interceptor - can add auth token here
  client.interceptors.request.use(
    (config) => {
      // Add auth token if available
      const token = localStorage.getItem('auth_token');
      if (token) {
        config.headers.Authorization = `Bearer ${token}`;
      }
      return config;
    },
    (error) => {
      return Promise.reject(error);
    }
  );

  // Response interceptor - handle errors globally
  client.interceptors.response.use(
    (response) => response,
    (error: AxiosError<ApiError | ValidationError>) => {
      // Handle different error types
      if (error.response) {
        const { status, data } = error.response;

        // Handle 401 Unauthorized
        if (status === 401) {
          localStorage.removeItem('auth_token');
          window.location.href = '/login';
        }

        // Handle validation errors
        if (status === 400 && data && 'details' in data) {
          return Promise.reject(data as ValidationError);
        }

        // Handle other API errors
        if (data && 'message' in data) {
          return Promise.reject(new Error(data.message));
        }
      }

      // Network error
      if (error.message === 'Network Error') {
        return Promise.reject(new Error('无法连接到服务器，请检查网络连接'));
      }

      return Promise.reject(error);
    }
  );

  return client;
};

// Export singleton instance
export const apiClient = createApiClient();

// Helper function for GET requests
export async function get<T>(url: string, config?: AxiosRequestConfig): Promise<T> {
  const response = await apiClient.get<T>(url, config);
  return response.data;
}

// Helper function for POST requests
export async function post<T>(url: string, data?: unknown, config?: AxiosRequestConfig): Promise<T> {
  const response = await apiClient.post<T>(url, data, config);
  return response.data;
}

// Helper function for PUT requests
export async function put<T>(url: string, data?: unknown, config?: AxiosRequestConfig): Promise<T> {
  const response = await apiClient.put<T>(url, data, config);
  return response.data;
}

// Helper function for DELETE requests
export async function del<T>(url: string, config?: AxiosRequestConfig): Promise<T> {
  const response = await apiClient.delete<T>(url, config);
  return response.data;
}
