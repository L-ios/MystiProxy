import React from 'react';
import { Card, Form, Input, InputNumber, Select, Button, Space, message, Spin, Divider } from 'antd';
import { SaveOutlined, ReloadOutlined } from '@ant-design/icons';
import { useSettings, useUpdateSettings } from '../../api/settings';
import type { SystemSettings } from '../../types/api';

const SettingsPage: React.FC = () => {
  const [form] = Form.useForm();
  const { data: settings, isLoading } = useSettings();
  const updateMutation = useUpdateSettings();

  React.useEffect(() => {
    if (settings) {
      form.setFieldsValue(settings);
    }
  }, [settings, form]);

  const handleSubmit = async (values: Partial<SystemSettings>) => {
    updateMutation.mutate(values, {
      onSuccess: () => {
        message.success('设置已保存');
      },
      onError: (error) => {
        message.error(`保存失败: ${(error as Error).message}`);
      },
    });
  };

  const handleReset = () => {
    if (settings) {
      form.setFieldsValue(settings);
    }
  };

  if (isLoading) {
    return (
      <Card>
        <Spin size="large" style={{ display: 'block', margin: '100px auto' }} />
      </Card>
    );
  }

  return (
    <Card title="系统设置">
      <Form
        form={form}
        layout="vertical"
        onFinish={handleSubmit}
        style={{ maxWidth: 600 }}
      >
        <Divider>同步配置</Divider>

        <Form.Item
          name="central_url"
          label="Central 服务器 URL"
          rules={[{ required: true, message: '请输入 Central 服务器 URL' }]}
        >
          <Input placeholder="例如: https://central.example.com" />
        </Form.Item>

        <Form.Item
          name="sync_interval"
          label="同步间隔 (秒)"
          rules={[{ required: true, message: '请输入同步间隔' }]}
          extra="自动同步配置的时间间隔，单位为秒"
        >
          <InputNumber min={10} max={3600} style={{ width: '100%' }} />
        </Form.Item>

        <Divider>日志配置</Divider>

        <Form.Item
          name="log_level"
          label="日志级别"
          rules={[{ required: true, message: '请选择日志级别' }]}
        >
          <Select
            options={[
              { label: 'Debug - 调试信息', value: 'debug' },
              { label: 'Info - 一般信息', value: 'info' },
              { label: 'Warn - 警告信息', value: 'warn' },
              { label: 'Error - 错误信息', value: 'error' },
            ]}
          />
        </Form.Item>

        <Divider>其他配置</Divider>

        <Form.Item
          name="max_request_history"
          label="最大请求历史记录数"
          extra="保留的请求历史记录数量，超过此数量将自动清理旧记录"
        >
          <InputNumber min={100} max={100000} step={100} style={{ width: '100%' }} />
        </Form.Item>

        <Form.Item
          name="default_environment"
          label="默认环境"
          extra="新建 Mock 时的默认环境"
        >
          <Input placeholder="例如: development" />
        </Form.Item>

        <Form.Item>
          <Space>
            <Button
              type="primary"
              htmlType="submit"
              icon={<SaveOutlined />}
              loading={updateMutation.isPending}
            >
              保存设置
            </Button>
            <Button icon={<ReloadOutlined />} onClick={handleReset}>
              重置
            </Button>
          </Space>
        </Form.Item>
      </Form>
    </Card>
  );
};

export default SettingsPage;
