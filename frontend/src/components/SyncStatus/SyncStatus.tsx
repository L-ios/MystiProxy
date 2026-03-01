import React from 'react';
import { Card, Tag, Button, Space, Tooltip, Spin, Typography } from 'antd';
import {
  SyncOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
  CloudSyncOutlined,
  ClockCircleOutlined,
} from '@ant-design/icons';
import { useSyncStatus, useSync } from '../../api/sync';
import dayjs from 'dayjs';
import relativeTime from 'dayjs/plugin/relativeTime';
import 'dayjs/locale/zh-cn';

dayjs.extend(relativeTime);
dayjs.locale('zh-cn');

const { Text } = Typography;

const SyncStatus: React.FC = () => {
  const { data: syncStatus, isLoading, isError, refetch } = useSyncStatus();
  const syncMutation = useSync();

  const handleSync = () => {
    syncMutation.mutate({ force: false });
  };

  const handleForceSync = () => {
    syncMutation.mutate({ force: true });
  };

  if (isLoading) {
    return (
      <Card size="small">
        <Spin size="small" />
      </Card>
    );
  }

  if (isError || !syncStatus) {
    return (
      <Card size="small">
        <Space>
          <CloseCircleOutlined style={{ color: '#ff4d4f' }} />
          <Text type="secondary">无法获取同步状态</Text>
          <Button size="small" onClick={() => refetch()}>
            重试
          </Button>
        </Space>
      </Card>
    );
  }

  const getStatusTag = () => {
    if (syncStatus.sync_in_progress) {
      return (
        <Tag icon={<SyncOutlined spin />} color="processing">
          同步中...
        </Tag>
      );
    }
    if (syncStatus.connected) {
      return (
        <Tag icon={<CheckCircleOutlined />} color="success">
          已连接
        </Tag>
      );
    }
    return (
      <Tag icon={<CloseCircleOutlined />} color="error">
        未连接
      </Tag>
    );
  };

  return (
    <Card
      size="small"
      title={
        <Space>
          <CloudSyncOutlined />
          <span>同步状态</span>
        </Space>
      }
      extra={
        <Space>
          <Tooltip title="立即同步">
            <Button
              type="primary"
              size="small"
              icon={<SyncOutlined spin={syncMutation.isPending} />}
              onClick={handleSync}
              loading={syncMutation.isPending}
              disabled={!syncStatus.connected || syncStatus.sync_in_progress}
            >
              同步
            </Button>
          </Tooltip>
          <Tooltip title="强制同步（覆盖本地变更）">
            <Button
              size="small"
              onClick={handleForceSync}
              loading={syncMutation.isPending}
              disabled={!syncStatus.connected || syncStatus.sync_in_progress}
            >
              强制同步
            </Button>
          </Tooltip>
        </Space>
      }
    >
      <Space direction="vertical" style={{ width: '100%' }}>
        <Space>
          <Text type="secondary">连接状态:</Text>
          {getStatusTag()}
        </Space>

        {syncStatus.last_sync_at && (
          <Space>
            <ClockCircleOutlined style={{ color: '#8c8c8c' }} />
            <Text type="secondary">
              最后同步: {dayjs(syncStatus.last_sync_at).fromNow()}
            </Text>
          </Space>
        )}

        {syncStatus.central_url && (
          <Space>
            <Text type="secondary">Central URL:</Text>
            <Text code>{syncStatus.central_url}</Text>
          </Space>
        )}

        {syncStatus.pending_changes > 0 && (
          <Tag color="warning">{syncStatus.pending_changes} 个待同步变更</Tag>
        )}

        {syncMutation.isSuccess && syncMutation.data && (
          <Tag color="success">
            同步完成: {syncMutation.data.synced_count} 个配置已同步
            {syncMutation.data.conflicts.length > 0 &&
              `, ${syncMutation.data.conflicts.length} 个冲突`}
          </Tag>
        )}

        {syncMutation.isError && (
          <Tag color="error">同步失败: {(syncMutation.error as Error).message}</Tag>
        )}
      </Space>
    </Card>
  );
};

export default SyncStatus;
