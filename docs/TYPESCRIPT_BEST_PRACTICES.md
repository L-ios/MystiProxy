# TypeScript 最佳实践指南

本文档汇总了 TypeScript 官方推荐的最佳实践资源，帮助开发者编写高质量的 TypeScript 代码。

---

## 核心资源

### 1. Effective TypeScript (Dan Vanderkam)

**书籍**: https://effectivetypescript.com/

**核心内容**:
- TypeScript 类型系统最佳实践
- 常见反模式和陷阱
- 类型设计模式

**关键要点**:

| 主题 | 内容 |
|------|------|
| **类型推断** | 优先让 TypeScript 推断类型，减少显式标注 |
| **any vs unknown** | 优先使用 `unknown`，比 `any` 更安全 |
| **类型收缩** | 使用类型守卫（type guards）缩小类型范围 |
| **泛型约束** | 合理使用泛型和约束，避免过度工程 |
| **接口 vs 类型** | 类相关使用 `interface`，其他使用 `type` |

**适用场景**:
- 想深入理解 TypeScript 类型系统
- 需要设计健壮的公共 API
- 避免 TypeScript 常见陷阱

---

### 2. Google TypeScript Style Guide

**文档**: http://google.github.io/styleguide/tsguide.html

**核心内容**:
- TypeScript 编码规范
- 命名约定
- 代码组织

**命名规范速查**:

| 类型 | 规范 | 示例 |
|------|------|------|
| 类/接口/类型 | PascalCase | `UserService`, `HttpClient` |
| 变量/函数/方法 | camelCase | `userData`, `getUser()` |
| 常量 | SCREAMING_SNAKE_CASE | `MAX_CONNECTIONS` |
| 文件名 | kebab-case | `user-service.ts` |

**核心规则**:
- ✅ 使用 `readonly` 标记不可变属性
- ✅ 优先使用 `interface` 定义对象类型
- ✅ 使用 `type` 定义联合类型、交叉类型
- ❌ 避免使用 `any`，使用 `unknown` 代替
- ❌ 不要使用 `Function` 类型

---

### 3. React + TypeScript 最佳实践

**核心内容**:
- React 组件类型定义
- Hooks 使用模式
- 状态管理类型设计

#### 组件类型定义

```typescript
// Props 接口命名: ComponentNameProps
interface ButtonProps {
  variant?: 'primary' | 'secondary' | 'danger';
  size?: 'small' | 'medium' | 'large';
  loading?: boolean;
  onClick?: () => void;
  children: React.ReactNode;
}

// 使用 forwardRef 处理 ref
const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ children, variant = 'primary', ...props }, ref) => {
    return <button ref={ref} {...props}>{children}</button>;
  }
);
```

#### Hooks 类型定义

```typescript
// useState 泛型
const [user, setUser] = useState<User | null>(null);

// useEffect 返回清理函数
useEffect(() => {
  const subscription = subscribe();
  return () => subscription.unsubscribe();
}, []);

// 自定义 Hook 返回类型
function useUser(userId: string): {
  user: User | null;
  loading: boolean;
  error: Error | null;
} {
  // ...
}
```

#### 状态管理类型设计

```typescript
// API 响应类型
interface ApiResponse<T> {
  data: T;
  message: string;
  success: boolean;
}

// 分页类型
interface Pagination {
  page: number;
  size: number;
  total: number;
}

// 状态类型
interface UserState {
  users: User[];
  loading: boolean;
  error: Error | null;
  pagination: Pagination;
}
```

---

## 快速参考

### 类型断言优先级

```
1. 首选类型推断           → const x = 3; // number
2. 显式类型标注           → let x: number = 3;
3. 类型断言 (as)          → x as number
4. 类型守卫               → if (x is string) { ... }
5. 双重断言 (避免使用)    → x as unknown as string
```

### any vs unknown

```typescript
// ❌ 避免 any
function processData(data: any) {
  data.foo(); // 没有类型检查
}

// ✅ 使用 unknown
function processData(data: unknown) {
  if (typeof data === 'object' && data !== null) {
    // 安全处理
  }
}

// ✅ 或使用具体类型
function processData(data: User) {
  data.name; // 类型安全
}
```

### 可选链与空值处理

```typescript
// ❌ 避免
const name = user && user.profile && user.profile.name;

// ✅ 使用可选链
const name = user?.profile?.name;

// ✅ 使用空值合并
const displayName = user?.profile?.name ?? 'Anonymous';
```

### 接口 vs 类型别名

```typescript
// ✅ 对象类型使用 interface
interface User {
  id: string;
  name: string;
}

// ✅ 联合类型、交叉类型使用 type
type Status = 'pending' | 'active' | 'inactive';
type ReadonlyUser = Readonly<User>;
type UserWithAge = User & { age: number };
```

---

## React Hooks 规则

### 基础规则

| 规则 | 说明 |
|------|------|
| **只在顶层调用** | 不在循环、条件、嵌套函数中调用 Hooks |
| **只在 React 函数中调用** | 函数组件或自定义 Hook 中 |
| **Hook 命名** | 以 `use` 开头 |

### 常用 Hooks 类型

```typescript
// useState
const [count, setCount] = useState<number>(0);

// useRef
const ref = useRef<HTMLDivElement>(null);

// useCallback
const handleClick = useCallback<(id: string) => void>(() => {
  // ...
}, [dependency]);

// useMemo
const sortedUsers = useMemo(() => {
  return users.sort((a, b) => a.name.localeCompare(b.name));
}, [users]);

// useEffect
useEffect(() => {
  const subscription = subscribe();
  return () => subscription.unsubscribe();
}, [dependency]);
```

---

## 项目结构

```
src/
├── components/      # React 组件
│   ├── common/     # 通用组件
│   └── ui/         # UI 组件
├── pages/          # 页面组件
├── hooks/          # 自定义 Hooks
├── services/       # API 服务
├── types/          # 类型定义
├── utils/          # 工具函数
├── store/          # 状态管理
└── constants/      # 常量
```

---

## 参考链接

### 官方资源
- [TypeScript Handbook](https://www.typescriptlang.org/docs/handbook/)
- [TypeScript Deep Dive](https://basarat.gitbook.io/typescript/)
- [React TypeScript Cheatsheets](https://react-typescript-cheatsheet.netlify.app/)

### 风格指南
- [Google TypeScript Style Guide](http://google.github.io/styleguide/tsguide.html)
- [Airbnb JavaScript Style Guide](https://github.com/airbnb/javascript)

### 工具链
- `npm run lint` - ESLint 代码检查
- `npm run type-check` - TypeScript 类型检查
- `prettier` - 代码格式化

---

**最后更新**: 2026-04-04
