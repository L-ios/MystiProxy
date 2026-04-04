import React, { useEffect } from 'react';
import {
  Form,
  Input,
  Select,
  InputNumber,
  Button,
  Card,
  Space,
  message,
  Divider,
} from 'antd';
import { SaveOutlined, ArrowLeftOutlined } from '@ant-design/icons';
import { useNavigate } from 'react-router-dom';
import { useCreateMock, useUpdateMock, useMock } from '../../api/mocks';
import type { MockCreateRequest, MockUpdateRequest, MatchingRules, ResponseConfig } from '../../types/api';

const { Option } = Select;

interface MockEditorProps {
  mode: 'create' | 'edit';
  mockId?: string;
}

const MockEditor: React.FC<MockEditorProps> = ({ mode, mockId }) => {
  const navigate = useNavigate();
  const [form] = Form.useForm();

  const { data: mockData, isLoading: isLoadingMock } = useMock(mockId || '');
  const createMock = useCreateMock();
  const updateMock = useUpdateMock(mockId || '');

  // Load mock data when editing
  useEffect(() => {
    if (mode === 'edit' && mockData) {
      form.setFieldsValue({
        name: mockData.name,
        path: mockData.path,
        method: mockData.method,
        matching_rules: mockData.matching_rules,
        response_config: mockData.response_config,
      });
    }
  }, [mode, mockData, form]);

  const handleSubmit = async (values: any) => {
    try {
      const matchingRules: MatchingRules = {
        path_pattern: values.matching_rules?.path_pattern,
        path_pattern_type: values.matching_rules?.path_pattern_type || 'exact',
        headers: values.matching_rules?.headers || [],
        query_params: values.matching_rules?.query_params || [],
        body: values.matching_rules?.body,
      };

      const responseConfig: ResponseConfig = {
        status: values.response_config?.status || 200,
        headers: values.response_config?.headers || {},
        body: values.response_config?.body,
        delay_ms: values.response_config?.delay_ms,
      };

      if (mode === 'create') {
        const createData: MockCreateRequest = {
          name: values.name,
          path: values.path,
          method: values.method,
          matching_rules: matchingRules,
          response_config: responseConfig,
        };
        await createMock.mutateAsync(createData);
        message.success('创建成功');
      } else {
        const updateData: MockUpdateRequest = {
          name: values.name,
          matching_rules: matchingRules,
          response_config: responseConfig,
        };
        await updateMock.mutateAsync(updateData);
        message.success('更新成功');
      }
      navigate('/mocks');
    } catch (error: any) {
      message.error(error.message || '操作失败');
    }
  };

  return (
    <Card
      title={mode === 'create' ? '新建 Mock 配置' : '编辑 Mock 配置'}
      loading={mode === 'edit' && isLoadingMock}
    >
      <Form
        form={form}
        layout="vertical"
        onFinish={handleSubmit}
        initialValues={{
          method: 'GET',
          matching_rules: {
            path_pattern_type: 'exact',
          },
          response_config: {
            status: 200,
          },
        }}
      >
        <Divider>基本信息</Divider>

        <Form.Item
          name="name"
          label="名称"
          rules={[{ required: true, message: '请输入名称' }]}
        >
          <Input placeholder="请输入 Mock 名称" />
        </Form.Item>

        <Form.Item
          name="path"
          label="路径"
          rules={[
            { required: true, message: '请输入路径' },
            { pattern: /^\//, message: '路径必须以 / 开头' },
          ]}
        >
          <Input placeholder="/api/users" disabled={mode === 'edit'} />
        </Form.Item>

        <Form.Item
          name="method"
          label="HTTP 方法"
          rules={[{ required: true, message: '请选择 HTTP 方法' }]}
        >
          <Select disabled={mode === 'edit'}>
            <Option value="GET">GET</Option>
            <Option value="POST">POST</Option>
            <Option value="PUT">PUT</Option>
            <Option value="DELETE">DELETE</Option>
            <Option value="PATCH">PATCH</Option>
          </Select>
        </Form.Item>

        <Divider>匹配规则</Divider>

        <Form.Item name={['matching_rules', 'path_pattern']} label="路径模式">
          <Input placeholder="路径匹配模式（可选）" />
        </Form.Item>

        <Form.Item
          name={['matching_rules', 'path_pattern_type']}
          label="路径匹配类型"
        >
          <Select>
            <Option value="exact">精确匹配</Option>
            <Option value="prefix">前缀匹配</Option>
            <Option value="regex">正则匹配</Option>
          </Select>
        </Form.Item>

        <Divider>响应配置</Divider>

        <Form.Item
          name={['response_config', 'status']}
          label="状态码"
          rules={[{ required: true, message: '请输入状态码' }]}
        >
          <InputNumber min={100} max={599} style={{ width: '100%' }} />
        </Form.Item>

        <Form.Item name={['response_config', 'delay_ms']} label="延迟 (毫秒)">
          <InputNumber min={0} max={60000} style={{ width: '100%' }} />
        </Form.Item>

        <Form.Item name={['response_config', 'body', 'type']} label="响应体类型">
          <Select>
            <Option value="static">静态内容</Option>
            <Option value="template">模板</Option>
            <Option value="file">文件</Option>
            <Option value="script">脚本</Option>
          </Select>
        </Form.Item>

        <Form.Item name={['response_config', 'body', 'content']} label="响应内容">
          <Input.TextArea
            rows={6}
            placeholder='{"code": 200, "message": "success", "data": {}}'
          />
        </Form.Item>

        <Form.Item style={{ marginTop: 24 }}>
          <Space>
            <Button
              type="primary"
              htmlType="submit"
              icon={<SaveOutlined />}
              loading={createMock.isPending || updateMock.isPending}
            >
              {mode === 'create' ? '创建' : '保存'}
            </Button>
            <Button
              icon={<ArrowLeftOutlined />}
              onClick={() => navigate('/mocks')}
            >
              返回
            </Button>
          </Space>
        </Form.Item>
      </Form>
    </Card>
  );
};

export default MockEditor;
