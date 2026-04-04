import React, { useState } from 'react';
import { Card, Table, Button, Space, Tag, Modal, Typography, Empty, Badge } from 'antd';
import {
  ExclamationCircleOutlined,
  EyeOutlined,
  ReloadOutlined,
} from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import { useConflicts, useDismissConflict } from '../../api/conflicts';
import ConflictResolver from '../../components/ConflictResolver';
import type { ConflictResponse } from '../../types/api';
import dayjs from 'dayjs';

const { Title } = Typography;

const ConflictsPage: React.FC = () => {
  const [selectedConflict, setSelectedConflict] = useState<ConflictResponse | null>(null);
  const [resolverVisible, setResolverVisible] = useState(false);

  const { data: conflictsData, isLoading, refetch } = useConflicts();
  const dismissMutation = useDismissConflict();

  const conflicts = conflictsData?.data || [];

  const handleViewConflict = (conflict: ConflictResponse) => {
    setSelectedConflict(conflict);
    setResolverVisible(true);
  };

  const handleDismiss = (configId: string) => {
    Modal.confirm({
      title: '确认忽略冲突?',
      icon: <ExclamationCircleOutlined />,
      content: '忽略冲突将保留两个版本，可能导致数据不一致。',
      okText: '确认',
      cancelText: '取消',
      onOk: () => {
        dismissMutation.mutate(configId);
      },
    });
  };

  const handleResolved = () => {
    setResolverVisible(false);
    setSelectedConflict(null);
    refetch();
  };

  const columns: ColumnsType<ConflictResponse> = [
    {
      title: '配置 ID',
      dataIndex: 'config_id',
      key: 'config_id',
      width: 120,
      ellipsis: true,
    },
    {
      title: '配置名称',
      key: 'name',
      render: (_, record) => record.local_version.name,
    },
    {
      title: '路径',
      key: 'path',
      render: (_, record) => (
        <Tag color="blue">
          {record.local_version.method} {record.local_version.path}
        </Tag>
      ),
    },
    {
      title: '本地版本更新时间',
      key: 'local_updated',
      render: (_, record) => dayjs(record.local_version.updated_at).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: 'Central 版本更新时间',
      key: 'central_updated',
      render: (_, record) => dayjs(record.central_version.updated_at).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: '检测时间',
      dataIndex: 'detected_at',
      key: 'detected_at',
      render: (date: string) => dayjs(date).format('YYYY-MM-DD HH:mm:ss'),
    },
    {
      title: '操作',
      key: 'action',
      width: 180,
      render: (_, record) => (
        <Space>
          <Button
            type="primary"
            size="small"
            icon={<EyeOutlined />}
            onClick={() => handleViewConflict(record)}
          >
            解决
          </Button>
          <Button size="small" onClick={() => handleDismiss(record.config_id)}>
            忽略
          </Button>
        </Space>
      ),
    },
  ];

  return (
    <div>
      <Card
        title={
          <Space>
            <Title level={4} style={{ margin: 0 }}>
              冲突管理
            </Title>
            <Badge count={conflicts.length} offset={[10, 0]} />
          </Space>
        }
        extra={
          <Button icon={<ReloadOutlined />} onClick={() => refetch()}>
            刷新
          </Button>
        }
      >
        {conflicts.length === 0 ? (
          <Empty description="暂无冲突" />
        ) : (
          <Table
            columns={columns}
            dataSource={conflicts}
            rowKey="config_id"
            loading={isLoading}
            pagination={false}
          />
        )}
      </Card>

      <Modal
        title="解决冲突"
        open={resolverVisible}
        onCancel={() => {
          setResolverVisible(false);
          setSelectedConflict(null);
        }}
        footer={null}
        width={900}
        destroyOnClose
      >
        {selectedConflict && (
          <ConflictResolver conflict={selectedConflict} onResolved={handleResolved} />
        )}
      </Modal>
    </div>
  );
};

export default ConflictsPage;
