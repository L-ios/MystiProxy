import React from 'react';
import { Card, Row, Col, Statistic, Spin, Alert } from 'antd';
import {
  ApiOutlined,
  CloudServerOutlined,
  CheckCircleOutlined,
  SyncOutlined,
  WarningOutlined,
} from '@ant-design/icons';
import { useMocks } from '../../api/mocks';
import { useInstances } from '../../api/instances';
import { useConflicts } from '../../api/conflicts';
import { useSyncStatus } from '../../api/sync';
import SyncStatus from '../../components/SyncStatus';
import ImportExport from '../../components/ImportExport';

const Dashboard: React.FC = () => {
  const { data: mocksData, isLoading: mocksLoading } = useMocks({ limit: 1 });
  const { data: instancesData, isLoading: instancesLoading } = useInstances();
  const { data: conflictsData, isLoading: conflictsLoading } = useConflicts();
  const { data: syncStatus, isLoading: syncLoading } = useSyncStatus();

  const totalMocks = mocksData?.pagination?.total || 0;
  const instances = instancesData?.data || [];
  const connectedInstances = instances.filter((i) => i.sync_status === 'connected').length;
  const totalConflicts = conflictsData?.total || 0;
  const syncPercentage = syncStatus?.connected ? 98 : 0;

  const isLoading = mocksLoading || instancesLoading || conflictsLoading || syncLoading;

  if (isLoading) {
    return (
      <Card>
        <Spin size="large" style={{ display: 'block', margin: '100px auto' }} />
      </Card>
    );
  }

  return (
    <div>
      {totalConflicts > 0 && (
        <Alert
          message={`您有 ${totalConflicts} 个待处理的冲突`}
          description="请前往冲突管理页面解决冲突，以确保配置同步正常。"
          type="warning"
          showIcon
          icon={<WarningOutlined />}
          style={{ marginBottom: 16 }}
          action={
            <a href="/conflicts">查看冲突</a>
          }
        />
      )}

      <Row gutter={[16, 16]}>
        <Col xs={24} sm={12} lg={6}>
          <Card>
            <Statistic
              title="Mock 总数"
              value={totalMocks}
              prefix={<ApiOutlined />}
              valueStyle={{ color: '#3f8600' }}
            />
          </Card>
        </Col>
        <Col xs={24} sm={12} lg={6}>
          <Card>
            <Statistic
              title="活跃实例"
              value={connectedInstances}
              suffix={`/ ${instances.length}`}
              prefix={<CloudServerOutlined />}
              valueStyle={{ color: '#1890ff' }}
            />
          </Card>
        </Col>
        <Col xs={24} sm={12} lg={6}>
          <Card>
            <Statistic
              title="同步状态"
              value={syncPercentage}
              prefix={<CheckCircleOutlined />}
              valueStyle={{ color: syncStatus?.connected ? '#52c41a' : '#ff4d4f' }}
              suffix="%"
            />
          </Card>
        </Col>
        <Col xs={24} sm={12} lg={6}>
          <Card>
            <Statistic
              title="待处理冲突"
              value={totalConflicts}
              prefix={<SyncOutlined />}
              valueStyle={{ color: totalConflicts > 0 ? '#faad14' : '#52c41a' }}
            />
          </Card>
        </Col>
      </Row>

      <Row gutter={[16, 16]} style={{ marginTop: 16 }}>
        <Col xs={24} lg={12}>
          <SyncStatus />
        </Col>
        <Col xs={24} lg={12}>
          <ImportExport />
        </Col>
      </Row>

      <Card title="快速开始" style={{ marginTop: 16 }}>
        <p>欢迎使用 MystiProxy Mock 管理系统!</p>
        <ul>
          <li>创建和管理 HTTP Mock 配置</li>
          <li>支持多环境管理</li>
          <li>实时同步到代理实例</li>
          <li>查看使用统计和分析</li>
          <li>支持配置导入导出</li>
        </ul>
      </Card>
    </div>
  );
};

export default Dashboard;
