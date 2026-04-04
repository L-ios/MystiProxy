import React from 'react';
import { Card, Table, Button, Space, Tag, Typography, Tooltip, Popconfirm, message } from 'antd';
import {
  ReloadOutlined,
  CloudUploadOutlined,
  DeleteOutlined,
  LinkOutlined,
  DisconnectOutlined,
  SyncOutlined,
  ExclamationCircleOutlined,
} from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import { useInstances, usePushConfig, usePushConfigAll, useUnregisterInstance } from '../../api/instances';
import type { MystiProxyInstance } from '../../types/api';
import dayjs from 'dayjs';
import relativeTime from 'dayjs/plugin/relativeTime';
import 'dayjs/locale/zh-cn';

dayjs.extend(relativeTime);
dayjs.locale('zh-cn');

const { Title } = Typography;

const InstancesPage: React.FC = () => {
  const { data: instancesData, isLoading, refetch } = useInstances();
  const pushConfigMutation = usePushConfig();
  const pushAllMutation = usePushConfigAll();
  const unregisterMutation = useUnregisterInstance();

  const instances = instancesData?.data || [];

  const handlePushConfig = (instanceId: string, instanceName: string) => {
    pushConfigMutation.mutate(instanceId, {
      onSuccess: () => {
        message.success(`配置已推送到 ${instanceName}`);
      },
      onError: (error) => {
        message.error(`推送失败: ${(error as Error).message}`);
      },
    });
  };

  const handlePushAll = () => {
    pushAllMutation.mutate(undefined, {
      onSuccess: () => {
        message.success('配置已推送到所有实例');
      },
      onError: (error) => {
        message.error(`推送失败: ${(error as Error).message}`);
      },
    });
  };

  const handleUnregister = (instanceId: string, instanceName: string) => {
    unregisterMutation.mutate(instanceId, {
      onSuccess: () => {
        message.success(`实例 ${instanceName} 已注销`);
      },
      onError: (error) => {
        message.error(`注销失败: ${(error as Error).message}`);
      },
    });
  };

  const getStatusTag = (status: MystiProxyInstance['sync_status']) => {
    const statusConfig: Record<
      MystiProxyInstance['sync_status'],
      { color: string; icon: React.ReactNode; text: string }
    > = {
      connected: { color: 'success', icon: <LinkOutlined />, text: '已连接' },
      disconnected: { color: 'error', icon: <DisconnectOutlined />, text: '已断开' },
      syncing: { color: 'processing', icon: <SyncOutlined spin />, text: '同步中' },
      conflict: { color: 'warning', icon: <ExclamationCircleOutlined />, text: '有冲突' },
    };

    const config = statusConfig[status];
    return (
      <Tag color={config.color} icon={config.icon}>
        {config.text}
      </Tag>
    );
  };

  const columns: ColumnsType<MystiProxyInstance> = [
    {
      title: '实例名称',
      dataIndex: 'name',
      key: 'name',
      render: (name: string) => <strong>{name}</strong>,
    },
    {
      title: '实例 ID',
      dataIndex: 'id',
      key: 'id',
      width: 120,
      ellipsis: true,
    },
    {
      title: '端点 URL',
      dataIndex: 'endpoint_url',
      key: 'endpoint_url',
      ellipsis: true,
      render: (url: string) => (
        <Tooltip title={url}>
          <a href={url} target="_blank" rel="noopener noreferrer">
            {url}
          </a>
        </Tooltip>
      ),
    },
    {
      title: '状态',
      dataIndex: 'sync_status',
      key: 'sync_status',
      width: 100,
      render: (status: MystiProxyInstance['sync_status']) => getStatusTag(status),
    },
    {
      title: '最后同步',
      dataIndex: 'last_sync_at',
      key: 'last_sync_at',
      width: 150,
      render: (date: string) => (date ? dayjs(date).fromNow() : '-'),
    },
    {
      title: '最后心跳',
      dataIndex: 'last_heartbeat',
      key: 'last_heartbeat',
      width: 150,
      render: (date: string) => (date ? dayjs(date).fromNow() : '-'),
    },
    {
      title: '注册时间',
      dataIndex: 'registered_at',
      key: 'registered_at',
      width: 180,
      render: (date: string) => dayjs(date).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: '操作',
      key: 'action',
      width: 200,
      render: (_, record) => (
        <Space>
          <Tooltip title="推送配置">
            <Button
              type="primary"
              size="small"
              icon={<CloudUploadOutlined />}
              onClick={() => handlePushConfig(record.id, record.name)}
              loading={pushConfigMutation.isPending}
              disabled={record.sync_status === 'disconnected'}
            >
              推送
            </Button>
          </Tooltip>
          <Popconfirm
            title="确认注销此实例?"
            description="注销后实例需要重新注册"
            onConfirm={() => handleUnregister(record.id, record.name)}
            okText="确认"
            cancelText="取消"
          >
            <Button size="small" danger icon={<DeleteOutlined />}>
              注销
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  return (
    <div>
      <Card
        title={
          <Title level={4} style={{ margin: 0 }}>
            实例管理
          </Title>
        }
        extra={
          <Space>
            <Button
              type="primary"
              icon={<CloudUploadOutlined />}
              onClick={handlePushAll}
              loading={pushAllMutation.isPending}
            >
              推送配置到所有实例
            </Button>
            <Button icon={<ReloadOutlined />} onClick={() => refetch()}>
              刷新
            </Button>
          </Space>
        }
      >
        <Table
          columns={columns}
          dataSource={instances}
          rowKey="id"
          loading={isLoading}
          pagination={{
            pageSize: 10,
            showSizeChanger: true,
            showTotal: (total) => `共 ${total} 个实例`,
          }}
        />
      </Card>
    </div>
  );
};

export default InstancesPage;
