# MystiProxy Mock Management Frontend

HTTP Mock 管理系统的前端管理界面

## 技术栈

- **React 18** - UI 框架
- **TypeScript** - 类型安全
- **Vite** - 构建工具
- **Ant Design 5** - UI 组件库
- **React Query** - 数据获取和状态管理
- **Zustand** - 轻量级状态管理
- **React Router 6** - 路由管理
- **Axios** - HTTP 客户端

## 项目结构

```
frontend/
├── src/
│   ├── api/                    # API 相关
│   │   ├── client.ts          # API 客户端封装
│   │   └── mocks.ts           # Mock API hooks
│   ├── components/             # 可复用组件
│   │   ├── Layout/            # 布局组件
│   │   │   └── MainLayout.tsx
│   │   ├── MockList/          # Mock 列表组件
│   │   │   └── MockList.tsx
│   │   └── MockEditor/        # Mock 编辑器组件
│   │       └── MockEditor.tsx
│   ├── pages/                  # 页面组件
│   │   ├── Dashboard/         # 仪表盘页面
│   │   │   └── Dashboard.tsx
│   │   └── Mocks/             # Mock 相关页面
│   │       ├── MocksPage.tsx
│   │       ├── MockCreatePage.tsx
│   │       └── MockEditPage.tsx
│   ├── stores/                 # 状态管理
│   │   └── mockStore.ts       # Mock 状态
│   ├── types/                  # TypeScript 类型定义
│   │   └── api.ts             # API 类型
│   ├── App.tsx                # 根组件
│   ├── main.tsx               # 入口文件
│   └── index.css              # 全局样式
├── .env                        # 环境变量
├── package.json
├── tsconfig.json
└── vite.config.ts
```

## 快速开始

### 安装依赖

```bash
npm install
```

### 配置环境变量

创建 `.env` 文件:

```env
VITE_API_BASE_URL=http://localhost:8080/api/v1
```

### 启动开发服务器

```bash
npm run dev
```

应用将在 http://localhost:5173 启动

### 构建生产版本

```bash
npm run build
```

### 预览生产版本

```bash
npm run preview
```

## 功能特性

### 已实现功能

- ✅ 基础布局和路由
- ✅ Mock 列表展示
- ✅ Mock 创建表单
- ✅ Mock 编辑表单
- ✅ Mock 删除功能
- ✅ Mock 批量删除
- ✅ Mock 搜索和过滤
- ✅ 分页功能
- ✅ API 客户端封装
- ✅ React Query 数据管理
- ✅ 错误处理

### 待实现功能

- 🚧 环境管理
- 🚧 实例管理
- 🚧 数据分析
- 🚧 系统设置
- 🚧 用户认证
- 🚧 冲突解决 UI

## API 集成

前端通过 REST API 与后端通信,API 契约基于 OpenAPI 规范:

- API 文档: `specs/001-mock-management/contracts/openapi.yaml`
- API 基础路径: `/api/v1`

### 主要 API 端点

- `GET /mocks` - 获取 Mock 列表
- `POST /mocks` - 创建 Mock
- `GET /mocks/:id` - 获取 Mock 详情
- `PUT /mocks/:id` - 更新 Mock
- `DELETE /mocks/:id` - 删除 Mock

## 开发指南

### 代码规范

- 使用 TypeScript 严格模式
- 遵循 React 函数组件最佳实践
- 使用 React Query 管理服务端状态
- 使用 Zustand 管理客户端状态
- 组件化开发,保持组件职责单一

### 组件开发

```typescript
// 组件示例
import React from 'react';
import { Card } from 'antd';

interface MyComponentProps {
  title: string;
}

const MyComponent: React.FC<MyComponentProps> = ({ title }) => {
  return <Card title={title}>Content</Card>;
};

export default MyComponent;
```

### API 调用

```typescript
// 使用 React Query hooks
import { useMocks } from './api/mocks';

const MyComponent = () => {
  const { data, isLoading, error } = useMocks({ page: 1, limit: 20 });

  if (isLoading) return <div>Loading...</div>;
  if (error) return <div>Error: {error.message}</div>;

  return <div>{/* render data */}</div>;
};
```

## 浏览器支持

- Chrome >= 87
- Firefox >= 78
- Safari >= 14
- Edge >= 88

## 许可证

MIT
