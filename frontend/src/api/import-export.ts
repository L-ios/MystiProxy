import { useMutation } from '@tanstack/react-query';
import { post } from './client';
import type {
  ExportRequest,
  ExportResponse,
  ImportRequest,
  ImportResponse,
} from '../types/api';

// Export configuration
export function useExportConfig() {
  return useMutation({
    mutationFn: async (data: ExportRequest) => {
      const response = await post<ExportResponse>('/export', data);
      // Create download
      const blob = new Blob([response.data], {
        type: data.format === 'json' ? 'application/json' : 'text/yaml',
      });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = response.filename;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      return response;
    },
  });
}

// Import configuration
export function useImportConfig() {
  return useMutation({
    mutationFn: (data: ImportRequest) => post<ImportResponse>('/import', data),
  });
}

// Helper function to read file content
export function readFileContent(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result as string);
    reader.onerror = () => reject(new Error('Failed to read file'));
    reader.readAsText(file);
  });
}
