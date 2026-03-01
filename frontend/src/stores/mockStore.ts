import { create } from 'zustand';
import type { MockConfiguration, MockFilter } from '../types/api';

interface MockState {
  // Filter state
  filter: MockFilter;
  setFilter: (filter: Partial<MockFilter>) => void;
  resetFilter: () => void;

  // Selected mock for editing
  selectedMock: MockConfiguration | null;
  setSelectedMock: (mock: MockConfiguration | null) => void;

  // UI state
  isEditorOpen: boolean;
  setEditorOpen: (open: boolean) => void;
}

const initialFilter: MockFilter = {
  page: 1,
  limit: 20,
};

export const useMockStore = create<MockState>((set) => ({
  // Filter state
  filter: initialFilter,
  setFilter: (filter) =>
    set((state) => ({
      filter: { ...state.filter, ...filter },
    })),
  resetFilter: () => set({ filter: initialFilter }),

  // Selected mock
  selectedMock: null,
  setSelectedMock: (mock) => set({ selectedMock: mock }),

  // UI state
  isEditorOpen: false,
  setEditorOpen: (open) => set({ isEditorOpen: open }),
}));
