import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { get, post, put, del } from './client';
import type {
  User,
  UserCreateRequest,
  UserUpdateRequest,
  UserListResponse,
  LoginRequest,
  LoginResponse,
  ChangePasswordRequest,
} from '../types/api';

// Query keys
const QUERY_KEYS = {
  users: ['users'] as const,
  user: (id: string) => ['user', id] as const,
  currentUser: ['user', 'current'] as const,
};

// Fetch user list
export function useUsers() {
  return useQuery({
    queryKey: QUERY_KEYS.users,
    queryFn: () => get<UserListResponse>('/users'),
  });
}

// Fetch single user
export function useUser(id: string) {
  return useQuery({
    queryKey: QUERY_KEYS.user(id),
    queryFn: () => get<User>(`/users/${id}`),
    enabled: !!id,
  });
}

// Fetch current user
export function useCurrentUser() {
  return useQuery({
    queryKey: QUERY_KEYS.currentUser,
    queryFn: () => get<User>('/users/me'),
    retry: false,
  });
}

// Create user
export function useCreateUser() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: UserCreateRequest) => post<User>('/users', data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.users });
    },
  });
}

// Update user
export function useUpdateUser(id: string) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: UserUpdateRequest) => put<User>(`/users/${id}`, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.users });
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.user(id) });
    },
  });
}

// Delete user
export function useDeleteUser() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (id: string) => del<void>(`/users/${id}`),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: QUERY_KEYS.users });
    },
  });
}

// Login
export function useLogin() {
  return useMutation({
    mutationFn: (data: LoginRequest) => post<LoginResponse>('/auth/login', data),
    onSuccess: (response) => {
      localStorage.setItem('auth_token', response.token);
      localStorage.setItem('user', JSON.stringify(response.user));
    },
  });
}

// Logout
export function useLogout() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      try {
        await post<void>('/auth/logout');
      } catch {
        // Ignore errors on logout
      }
    },
    onSuccess: () => {
      localStorage.removeItem('auth_token');
      localStorage.removeItem('user');
      queryClient.clear();
    },
  });
}

// Change password
export function useChangePassword() {
  return useMutation({
    mutationFn: (data: ChangePasswordRequest) => post<void>('/users/me/password', data),
  });
}
