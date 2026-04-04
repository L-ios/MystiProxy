import React, { useState } from 'react';
import { Layout, Menu, theme, Dropdown, Avatar, Space, Typography, Badge } from 'antd';
import {
  DashboardOutlined,
  ApiOutlined,
  CloudServerOutlined,
  BarChartOutlined,
  SettingOutlined,
  WarningOutlined,
  UserOutlined,
  LogoutOutlined,
  TeamOutlined,
  AlertOutlined,
  EnvironmentOutlined,
} from '@ant-design/icons';
import { useNavigate, useLocation } from 'react-router-dom';
import { useConflicts } from '../../api/conflicts';
import { useLogout, useCurrentUser } from '../../api/users';

const { Header, Sider, Content } = Layout;
const { Text } = Typography;

interface MainLayoutProps {
  children: React.ReactNode;
}

const MainLayout: React.FC<MainLayoutProps> = ({ children }) => {
  const [collapsed, setCollapsed] = useState(false);
  const navigate = useNavigate();
  const location = useLocation();
  const { data: conflictsData } = useConflicts();
  const { data: user } = useCurrentUser();
  const logoutMutation = useLogout();

  const {
    token: { colorBgContainer, borderRadiusLG },
  } = theme.useToken();

  const conflictCount = conflictsData?.total || 0;

  const handleLogout = () => {
    logoutMutation.mutate(undefined, {
      onSuccess: () => {
        navigate('/login');
      },
    });
  };

  const userMenuItems = [
    {
      key: 'profile',
      icon: <UserOutlined />,
      label: '个人信息',
    },
    {
      key: 'logout',
      icon: <LogoutOutlined />,
      label: '退出登录',
      danger: true,
      onClick: handleLogout,
    },
  ];

  const menuItems = [
    {
      key: '/',
      icon: <DashboardOutlined />,
      label: '仪表盘',
    },
    {
      key: '/mocks',
      icon: <ApiOutlined />,
      label: 'Mock 管理',
    },
    {
      key: '/environments',
      icon: <EnvironmentOutlined />,
      label: '环境管理',
    },
    {
      key: '/instances',
      icon: <CloudServerOutlined />,
      label: '实例管理',
    },
    {
      key: '/conflicts',
      icon: (
        <Badge count={conflictCount} size="small" offset={[6, 0]}>
          <AlertOutlined />
        </Badge>
      ),
      label: '冲突管理',
    },
    {
      key: '/analytics',
      icon: <BarChartOutlined />,
      label: '数据分析',
    },
    {
      key: '/users',
      icon: <TeamOutlined />,
      label: '用户管理',
    },
    {
      key: '/settings',
      icon: <SettingOutlined />,
      label: '系统设置',
    },
  ];

  const handleMenuClick = ({ key }: { key: string }) => {
    navigate(key);
  };

  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Sider
        collapsible
        collapsed={collapsed}
        onCollapse={(value) => setCollapsed(value)}
        style={{
          overflow: 'auto',
          height: '100vh',
          position: 'fixed',
          left: 0,
          top: 0,
          bottom: 0,
        }}
      >
        <div
          style={{
            height: 32,
            margin: 16,
            background: 'rgba(255, 255, 255, 0.2)',
            borderRadius: borderRadiusLG,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            color: '#fff',
            fontSize: collapsed ? 16 : 18,
            fontWeight: 'bold',
          }}
        >
          {collapsed ? 'MP' : 'MystiProxy'}
        </div>
        <Menu
          theme="dark"
          mode="inline"
          selectedKeys={[location.pathname]}
          items={menuItems}
          onClick={handleMenuClick}
        />
      </Sider>
      <Layout style={{ marginLeft: collapsed ? 80 : 200, transition: 'all 0.2s' }}>
        <Header
          style={{
            padding: '0 24px',
            background: colorBgContainer,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            borderBottom: '1px solid #f0f0f0',
          }}
        >
          <h2 style={{ margin: 0, fontSize: 18, fontWeight: 600 }}>
            HTTP Mock 管理系统
          </h2>
          <Space>
            {conflictCount > 0 && (
              <Badge count={conflictCount}>
                <WarningOutlined
                  style={{ fontSize: 18, color: '#faad14', cursor: 'pointer' }}
                  onClick={() => navigate('/conflicts')}
                />
              </Badge>
            )}
            <Dropdown menu={{ items: userMenuItems }} placement="bottomRight">
              <Space style={{ cursor: 'pointer' }}>
                <Avatar icon={<UserOutlined />} style={{ backgroundColor: '#1890ff' }} />
                <Text>{user?.username || '用户'}</Text>
              </Space>
            </Dropdown>
          </Space>
        </Header>
        <Content
          style={{
            margin: '24px 16px',
            padding: 24,
            background: colorBgContainer,
            borderRadius: borderRadiusLG,
            minHeight: 280,
            overflow: 'auto',
          }}
        >
          {children}
        </Content>
      </Layout>
    </Layout>
  );
};

export default MainLayout;
