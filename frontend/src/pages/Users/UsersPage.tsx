import React, { useState } from 'react';
import {
  Card,
  Table,
  Button,
  Space,
  Tag,
  Typography,
  Modal,
  Form,
  Input,
  Select,
  message,
  Dropdown,
} from 'antd';
import {
  PlusOutlined,
  EditOutlined,
  DeleteOutlined,
  ReloadOutlined,
  MoreOutlined,
  KeyOutlined,
} from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import { useQueryClient } from '@tanstack/react-query';
import { useUsers } from '../../api/users';
import { post, put, del } from '../../api/client';
import type { User, UserCreateRequest, UserUpdateRequest } from '../../types/api';
import dayjs from 'dayjs';

const { Title } = Typography;

const UsersPage: React.FC = () => {
  const [form] = Form.useForm();
  const [modalVisible, setModalVisible] = useState(false);
  const [editingUser, setEditingUser] = useState<User | null>(null);
  const [passwordModalVisible, setPasswordModalVisible] = useState(false);
  const [selectedUserId, setSelectedUserId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const { data: usersData, isLoading, refetch } = useUsers();
  const queryClient = useQueryClient();

  const users = usersData?.data || [];

  const handleCreate = () => {
    setEditingUser(null);
    form.resetFields();
    setModalVisible(true);
  };

  const handleEdit = (user: User) => {
    setEditingUser(user);
    form.setFieldsValue({
      username: user.username,
      email: user.email,
      role: user.role,
      team_id: user.team_id,
    });
    setModalVisible(true);
  };

  const handleDelete = async (id: string, username: string) => {
    try {
      await del<void>(`/users/${id}`);
      message.success(`用户 ${username} 已删除`);
      queryClient.invalidateQueries({ queryKey: ['users'] });
    } catch (error) {
      message.error(`删除失败: ${(error as Error).message}`);
    }
  };

  const handleSubmit = async () => {
    try {
      setLoading(true);
      const values = await form.validateFields();

      if (editingUser) {
        const data: UserUpdateRequest = {
          username: values.username,
          email: values.email,
          role: values.role,
          team_id: values.team_id,
        };
        await put<User>(`/users/${editingUser.id}`, data);
        message.success('用户更新成功');
      } else {
        const data: UserCreateRequest = {
          username: values.username,
          email: values.email,
          password: values.password,
          role: values.role,
          team_id: values.team_id,
        };
        await post<User>('/users', data);
        message.success('用户创建成功');
      }

      setModalVisible(false);
      form.resetFields();
      queryClient.invalidateQueries({ queryKey: ['users'] });
    } catch (error) {
      message.error(`操作失败: ${(error as Error).message}`);
    } finally {
      setLoading(false);
    }
  };

  const handleResetPassword = (userId: string) => {
    setSelectedUserId(userId);
    setPasswordModalVisible(true);
  };

  const confirmResetPassword = async () => {
    if (!selectedUserId) return;
    try {
      await post<void>(`/users/${selectedUserId}/reset-password`);
      message.success('密码已重置为默认密码');
      setPasswordModalVisible(false);
      setSelectedUserId(null);
    } catch (error) {
      message.error(`重置失败: ${(error as Error).message}`);
    }
  };

  const getRoleTag = (role: User['role']) => {
    const roleConfig: Record<User['role'], { color: string; text: string }> = {
      admin: { color: 'red', text: '管理员' },
      user: { color: 'blue', text: '用户' },
      viewer: { color: 'green', text: '访客' },
    };
    const config = roleConfig[role];
    return <Tag color={config.color}>{config.text}</Tag>;
  };

  const columns: ColumnsType<User> = [
    {
      title: '用户名',
      dataIndex: 'username',
      key: 'username',
      render: (name: string) => <strong>{name}</strong>,
    },
    {
      title: '邮箱',
      dataIndex: 'email',
      key: 'email',
    },
    {
      title: '角色',
      dataIndex: 'role',
      key: 'role',
      width: 100,
      render: (role: User['role']) => getRoleTag(role),
    },
    {
      title: '团队',
      dataIndex: 'team_id',
      key: 'team_id',
      render: (teamId: string) => teamId || '-',
    },
    {
      title: '最后登录',
      dataIndex: 'last_login_at',
      key: 'last_login_at',
      width: 180,
      render: (date: string) => (date ? dayjs(date).format('YYYY-MM-DD HH:mm:ss') : '-'),
    },
    {
      title: '创建时间',
      dataIndex: 'created_at',
      key: 'created_at',
      width: 180,
      render: (date: string) => dayjs(date).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: '操作',
      key: 'action',
      width: 150,
      render: (_, record) => (
        <Space>
          <Button size="small" icon={<EditOutlined />} onClick={() => handleEdit(record)}>
            编辑
          </Button>
          <Dropdown
            menu={{
              items: [
                {
                  key: 'password',
                  icon: <KeyOutlined />,
                  label: '重置密码',
                  onClick: () => handleResetPassword(record.id),
                },
                {
                  key: 'delete',
                  icon: <DeleteOutlined />,
                  label: '删除用户',
                  danger: true,
                  onClick: () => {
                    Modal.confirm({
                      title: '确认删除此用户?',
                      content: `用户: ${record.username}`,
                      okText: '确认',
                      cancelText: '取消',
                      onOk: () => handleDelete(record.id, record.username),
                    });
                  },
                },
              ],
            }}
          >
            <Button size="small" icon={<MoreOutlined />} />
          </Dropdown>
        </Space>
      ),
    },
  ];

  return (
    <div>
      <Card
        title={
          <Title level={4} style={{ margin: 0 }}>
            用户管理
          </Title>
        }
        extra={
          <Space>
            <Button type="primary" icon={<PlusOutlined />} onClick={handleCreate}>
              新建用户
            </Button>
            <Button icon={<ReloadOutlined />} onClick={() => refetch()}>
              刷新
            </Button>
          </Space>
        }
      >
        <Table
          columns={columns}
          dataSource={users}
          rowKey="id"
          loading={isLoading}
          pagination={{
            pageSize: 10,
            showSizeChanger: true,
            showTotal: (total) => `共 ${total} 个用户`,
          }}
        />
      </Card>

      {/* Create/Edit Modal */}
      <Modal
        title={editingUser ? '编辑用户' : '新建用户'}
        open={modalVisible}
        onOk={handleSubmit}
        onCancel={() => {
          setModalVisible(false);
          form.resetFields();
        }}
        confirmLoading={loading}
        destroyOnClose
      >
        <Form form={form} layout="vertical">
          <Form.Item
            name="username"
            label="用户名"
            rules={[{ required: true, message: '请输入用户名' }]}
          >
            <Input placeholder="用户名" />
          </Form.Item>
          <Form.Item
            name="email"
            label="邮箱"
            rules={[
              { required: true, message: '请输入邮箱' },
              { type: 'email', message: '请输入有效的邮箱地址' },
            ]}
          >
            <Input placeholder="邮箱" />
          </Form.Item>
          {!editingUser && (
            <Form.Item
              name="password"
              label="密码"
              rules={[
                { required: true, message: '请输入密码' },
                { min: 6, message: '密码至少 6 个字符' },
              ]}
            >
              <Input.Password placeholder="密码" />
            </Form.Item>
          )}
          <Form.Item
            name="role"
            label="角色"
            rules={[{ required: true, message: '请选择角色' }]}
          >
            <Select
              placeholder="选择角色"
              options={[
                { label: '管理员', value: 'admin' },
                { label: '用户', value: 'user' },
                { label: '访客', value: 'viewer' },
              ]}
            />
          </Form.Item>
          <Form.Item name="team_id" label="团队">
            <Input placeholder="团队 ID (可选)" />
          </Form.Item>
        </Form>
      </Modal>

      {/* Reset Password Modal */}
      <Modal
        title="重置密码"
        open={passwordModalVisible}
        onCancel={() => {
          setPasswordModalVisible(false);
          setSelectedUserId(null);
        }}
        onOk={confirmResetPassword}
      >
        <p>确认重置此用户的密码?</p>
        <p>密码将被重置为默认密码: <code>123456</code></p>
      </Modal>
    </div>
  );
};

export default UsersPage;
