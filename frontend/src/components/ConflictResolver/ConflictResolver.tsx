import React, { useState } from 'react';
import {
  Card,
  Tabs,
  Button,
  Space,
  Typography,
  Tag,
  Descriptions,
  Alert,
  Radio,
  Input,
  message,
  Spin,
} from 'antd';
import {
  CheckCircleOutlined,
  MergeCellsOutlined,
  ArrowRightOutlined,
} from '@ant-design/icons';
import type { ConflictResponse, MockConfiguration, ConflictResolveRequest } from '../../types/api';
import { useResolveConflict } from '../../api/conflicts';
import dayjs from 'dayjs';

const { Text, Title } = Typography;
const { TextArea } = Input;

interface ConflictResolverProps {
  conflict: ConflictResponse;
  onResolved?: () => void;
}

const ConflictResolver: React.FC<ConflictResolverProps> = ({ conflict, onResolved }) => {
  const [resolution, setResolution] = useState<'keep_local' | 'keep_central' | 'merge'>('keep_local');
  const [mergedConfig, setMergedConfig] = useState<string>('');
  const resolveMutation = useResolveConflict();

  const handleResolve = () => {
    const data: ConflictResolveRequest = {
      resolution,
    };

    if (resolution === 'merge' && mergedConfig) {
      try {
        data.merged_config = JSON.parse(mergedConfig);
      } catch {
        message.error('合并配置 JSON 格式错误');
        return;
      }
    }

    resolveMutation.mutate(
      { configId: conflict.config_id, data },
      {
        onSuccess: () => {
          message.success('冲突已解决');
          onResolved?.();
        },
        onError: (error) => {
          message.error(`解决失败: ${(error as Error).message}`);
        },
      }
    );
  };

  const renderConfigDiff = (local: MockConfiguration, central: MockConfiguration) => {
    const differences: Array<{ field: string; local: unknown; central: unknown }> = [];

    if (local.name !== central.name) {
      differences.push({ field: '名称', local: local.name, central: central.name });
    }
    if (local.path !== central.path) {
      differences.push({ field: '路径', local: local.path, central: central.path });
    }
    if (local.method !== central.method) {
      differences.push({ field: '方法', local: local.method, central: central.method });
    }
    if (local.response_config.status !== central.response_config.status) {
      differences.push({
        field: '响应状态码',
        local: local.response_config.status,
        central: central.response_config.status,
      });
    }
    if (JSON.stringify(local.matching_rules) !== JSON.stringify(central.matching_rules)) {
      differences.push({
        field: '匹配规则',
        local: JSON.stringify(local.matching_rules, null, 2),
        central: JSON.stringify(central.matching_rules, null, 2),
      });
    }
    if (JSON.stringify(local.response_config) !== JSON.stringify(central.response_config)) {
      differences.push({
        field: '响应配置',
        local: JSON.stringify(local.response_config, null, 2),
        central: JSON.stringify(central.response_config, null, 2),
      });
    }

    return differences;
  };

  const differences = renderConfigDiff(conflict.local_version, conflict.central_version);

  const renderMockDetails = (config: MockConfiguration) => (
    <Descriptions column={2} size="small" bordered>
      <Descriptions.Item label="名称">{config.name}</Descriptions.Item>
      <Descriptions.Item label="路径">{config.path}</Descriptions.Item>
      <Descriptions.Item label="方法">
        <Tag color="blue">{config.method}</Tag>
      </Descriptions.Item>
      <Descriptions.Item label="状态码">
        <Tag color={config.response_config.status < 400 ? 'success' : 'error'}>
          {config.response_config.status}
        </Tag>
      </Descriptions.Item>
      <Descriptions.Item label="来源">
        <Tag color={config.source === 'central' ? 'purple' : 'green'}>{config.source}</Tag>
      </Descriptions.Item>
      <Descriptions.Item label="更新时间">
        {dayjs(config.updated_at).format('YYYY-MM-DD HH:mm:ss')}
      </Descriptions.Item>
      <Descriptions.Item label="匹配规则" span={2}>
        <pre style={{ margin: 0, maxHeight: 150, overflow: 'auto', fontSize: 12 }}>
          {JSON.stringify(config.matching_rules, null, 2)}
        </pre>
      </Descriptions.Item>
      <Descriptions.Item label="响应配置" span={2}>
        <pre style={{ margin: 0, maxHeight: 150, overflow: 'auto', fontSize: 12 }}>
          {JSON.stringify(config.response_config, null, 2)}
        </pre>
      </Descriptions.Item>
    </Descriptions>
  );

  return (
    <Card>
      <Alert
        message="检测到配置冲突"
        description={`配置 "${conflict.local_version.name}" 在本地和 Central 服务器上有不同的修改。请选择解决方式。`}
        type="warning"
        showIcon
        style={{ marginBottom: 16 }}
      />

      <Title level={5}>差异对比</Title>
      {differences.length > 0 ? (
        <div style={{ marginBottom: 16 }}>
          {differences.map((diff, index) => (
            <Card key={index} size="small" style={{ marginBottom: 8 }}>
              <Text strong>{diff.field}</Text>
              <div style={{ display: 'flex', alignItems: 'center', gap: 16, marginTop: 8 }}>
                <div style={{ flex: 1 }}>
                  <Tag color="green">本地</Tag>
                  <pre style={{ margin: 0, fontSize: 12, whiteSpace: 'pre-wrap' }}>
                    {String(diff.local)}
                  </pre>
                </div>
                <ArrowRightOutlined style={{ color: '#999' }} />
                <div style={{ flex: 1 }}>
                  <Tag color="purple">Central</Tag>
                  <pre style={{ margin: 0, fontSize: 12, whiteSpace: 'pre-wrap' }}>
                    {String(diff.central)}
                  </pre>
                </div>
              </div>
            </Card>
          ))}
        </div>
      ) : (
        <Alert message="未检测到明显差异，可能是版本向量冲突" type="info" style={{ marginBottom: 16 }} />
      )}

      <Tabs
        items={[
          {
            key: 'local',
            label: (
              <span>
                <Tag color="green">本地版本</Tag>
              </span>
            ),
            children: renderMockDetails(conflict.local_version),
          },
          {
            key: 'central',
            label: (
              <span>
                <Tag color="purple">Central 版本</Tag>
              </span>
            ),
            children: renderMockDetails(conflict.central_version),
          },
        ]}
      />

      <Card title="选择解决方式" style={{ marginTop: 16 }}>
        <Radio.Group
          value={resolution}
          onChange={(e) => setResolution(e.target.value)}
          style={{ width: '100%' }}
        >
          <Space direction="vertical" style={{ width: '100%' }}>
            <Radio value="keep_local">
              <Space>
                <CheckCircleOutlined style={{ color: '#52c41a' }} />
                <Text strong>保留本地版本</Text>
              </Space>
              <br />
              <Text type="secondary" style={{ marginLeft: 24 }}>
                使用本地配置覆盖 Central 版本
              </Text>
            </Radio>

            <Radio value="keep_central">
              <Space>
                <CheckCircleOutlined style={{ color: '#722ed1' }} />
                <Text strong>使用 Central 版本</Text>
              </Space>
              <br />
              <Text type="secondary" style={{ marginLeft: 24 }}>
                放弃本地修改，使用 Central 版本
              </Text>
            </Radio>

            <Radio value="merge">
              <Space>
                <MergeCellsOutlined style={{ color: '#1890ff' }} />
                <Text strong>手动合并</Text>
              </Space>
              <br />
              <Text type="secondary" style={{ marginLeft: 24 }}>
                手动编辑合并后的配置
              </Text>
            </Radio>
          </Space>
        </Radio.Group>

        {resolution === 'merge' && (
          <div style={{ marginTop: 16 }}>
            <Text>合并后的配置 (JSON 格式):</Text>
            <TextArea
              rows={10}
              value={mergedConfig || JSON.stringify(conflict.local_version, null, 2)}
              onChange={(e) => setMergedConfig(e.target.value)}
              placeholder="输入合并后的配置 JSON"
              style={{ fontFamily: 'monospace', marginTop: 8 }}
            />
          </div>
        )}

        <div style={{ marginTop: 16, textAlign: 'right' }}>
          <Button
            type="primary"
            onClick={handleResolve}
            loading={resolveMutation.isPending}
            icon={resolveMutation.isPending ? <Spin size="small" /> : undefined}
          >
            确认解决
          </Button>
        </div>
      </Card>
    </Card>
  );
};

export default ConflictResolver;
