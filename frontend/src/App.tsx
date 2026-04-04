import React from 'react';
import { BrowserRouter as Router, Routes, Route, Navigate } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ConfigProvider } from 'antd';
import zhCN from 'antd/locale/zh_CN';

import MainLayout from './components/Layout/MainLayout';
import Dashboard from './pages/Dashboard/Dashboard';
import MocksPage from './pages/Mocks/MocksPage';
import MockCreatePage from './pages/Mocks/MockCreatePage';
import MockEditPage from './pages/Mocks/MockEditPage';
import EnvironmentsPage from './pages/Environments/EnvironmentsPage';
import InstancesPage from './pages/Instances/InstancesPage';
import AnalyticsPage from './pages/Analytics/AnalyticsPage';
import SettingsPage from './pages/Settings/SettingsPage';
import ConflictsPage from './pages/Conflicts/ConflictsPage';
import UsersPage from './pages/Users/UsersPage';
import LoginPage from './pages/Login/LoginPage';

// Create a client
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      retry: 1,
      staleTime: 5 * 60 * 1000, // 5 minutes
    },
  },
});

// Simple auth check
const isAuthenticated = () => {
  return !!localStorage.getItem('auth_token');
};

// Protected Route wrapper
const ProtectedRoute: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  if (!isAuthenticated()) {
    return <Navigate to="/login" replace />;
  }
  return <>{children}</>;
};

const App: React.FC = () => {
  return (
    <QueryClientProvider client={queryClient}>
      <ConfigProvider
        locale={zhCN}
        theme={{
          token: {
            colorPrimary: '#1890ff',
            borderRadius: 6,
          },
        }}
      >
        <Router>
          <Routes>
            {/* Login route - no layout */}
            <Route path="/login" element={<LoginPage />} />

            {/* Protected routes with layout */}
            <Route
              path="/"
              element={
                <ProtectedRoute>
                  <MainLayout>
                    <Dashboard />
                  </MainLayout>
                </ProtectedRoute>
              }
            />
            <Route
              path="/mocks"
              element={
                <ProtectedRoute>
                  <MainLayout>
                    <MocksPage />
                  </MainLayout>
                </ProtectedRoute>
              }
            />
            <Route
              path="/mocks/create"
              element={
                <ProtectedRoute>
                  <MainLayout>
                    <MockCreatePage />
                  </MainLayout>
                </ProtectedRoute>
              }
            />
            <Route
              path="/mocks/edit/:id"
              element={
                <ProtectedRoute>
                  <MainLayout>
                    <MockEditPage />
                  </MainLayout>
                </ProtectedRoute>
              }
            />
            <Route
              path="/environments"
              element={
                <ProtectedRoute>
                  <MainLayout>
                    <EnvironmentsPage />
                  </MainLayout>
                </ProtectedRoute>
              }
            />
            <Route
              path="/instances"
              element={
                <ProtectedRoute>
                  <MainLayout>
                    <InstancesPage />
                  </MainLayout>
                </ProtectedRoute>
              }
            />
            <Route
              path="/analytics"
              element={
                <ProtectedRoute>
                  <MainLayout>
                    <AnalyticsPage />
                  </MainLayout>
                </ProtectedRoute>
              }
            />
            <Route
              path="/settings"
              element={
                <ProtectedRoute>
                  <MainLayout>
                    <SettingsPage />
                  </MainLayout>
                </ProtectedRoute>
              }
            />
            <Route
              path="/conflicts"
              element={
                <ProtectedRoute>
                  <MainLayout>
                    <ConflictsPage />
                  </MainLayout>
                </ProtectedRoute>
              }
            />
            <Route
              path="/users"
              element={
                <ProtectedRoute>
                  <MainLayout>
                    <UsersPage />
                  </MainLayout>
                </ProtectedRoute>
              }
            />

            {/* Catch all - redirect to dashboard */}
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </Router>
      </ConfigProvider>
    </QueryClientProvider>
  );
};

export default App;
