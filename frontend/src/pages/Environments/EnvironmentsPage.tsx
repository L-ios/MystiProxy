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
  message,
  Popconfirm,
  Descriptions,
} from 'antd';
import {
  PlusOutlined,
  EditOutlined,
  DeleteOutlined,
  ReloadOutlined,
  GlobalOutlined,
} from '@ant-design/icons';
import type { ColumnsType } from 'antd/es/table';
import {
  useEnvironments,
  useCreateEnvironment,
  useDeleteEnvironment,
} from '../../api/environments';
import type { Environment, EnvironmentCreateRequest } from '../../types/api';
import dayjs from 'dayjs';

const { Title } = Typography;
const { TextArea } = Input;

const EnvironmentsPage: React.FC = () => {
  const [form] = Form.useForm();
  const [modalVisible, setModalVisible] = useState(false);
  const [editingEnv, setEditingEnv] = useState<Environment | null>(null);
  const [detailVisible, setDetailVisible] = useState(false);
  const [selectedEnv, setSelectedEnv] = useState<Environment | null>(null);

  const { data: envsData, isLoading, refetch } = useEnvironments();
  const createMutation = useCreateEnvironment();
  const deleteMutation = useDeleteEnvironment();

  const environments = envsData?.data || [];

  const handleCreate = () => {
    setEditingEnv(null);
    form.resetFields();
    setModalVisible(true);
  };

  const handleEdit = (env: Environment) => {
    setEditingEnv(env);
    form.setFieldsValue({
      name: env.name,
      description: env.description,
      endpoints: env.endpoints
        ? Object.entries(env.endpoints).map(([key, value]) => ({ key, value }))
        : [],
    });
    setModalVisible(true);
  };

  const handleViewDetail = (env: Environment) => {
    setSelectedEnv(env);
    setDetailVisible(true);
  };

  const handleDelete = (id: string, name: string) => {
    deleteMutation.mutate(id, {
      onSuccess: () => {
        message.success(`环境 ${name} 已删除`);
      },
      onError: (error) => {
        message.error(`删除失败: ${(error as Error).message}`);
      },
    });
  };

  const handleSubmit = async () => {
    try {
      const values = await form.validateFields();
      const endpoints: Record<string, string> = {};
      if (values.endpoints) {
        values.endpoints.forEach((item: { key: string; value: string }) => {
          if (item.key && item.value) {
            endpoints[item.key] = item.value;
          }
        });
      }

      const data: EnvironmentCreateRequest = {
        name: values.name,
        description: values.description,
        endpoints,
      };

      createMutation.mutate(data, {
        onSuccess: () => {
          message.success('环境创建成功');
          setModalVisible(false);
          form.resetFields();
        },
        onError: (error) => {
          message.error(`创建失败: ${(error as Error).message}`);
        },
      });
    } catch (error) {
      // Form validation error
    }
  };

  const columns: ColumnsType<Environment> = [
    {
      title: '环境名称',
      dataIndex: 'name',
      key: 'name',
      render: (name: string) => <strong>{name}</strong>,
    },
    {
      title: '描述',
      dataIndex: 'description',
      key: 'description',
      ellipsis: true,
      render: (desc: string) => desc || '-',
    },
    {
      title: '端点数量',
      key: 'endpoints_count',
      width: 100,
      render: (_, record) => (record.endpoints ? Object.keys(record.endpoints).length : 0),
    },
    {
      title: '模板',
      dataIndex: 'is_template',
      key: 'is_template',
      width: 80,
      render: (isTemplate: boolean) =>
        isTemplate ? <Tag color="blue">模板</Tag> : <Tag>环境</Tag>,
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
      width: 200,
      render: (_, record) => (
        <Space>
          <Button size="small" icon={<GlobalOutlined />} onClick={() => handleViewDetail(record)}>
            详情
          </Button>
          <Button size="small" icon={<EditOutlined />} onClick={() => handleEdit(record)}>
            编辑
          </Button>
          <Popconfirm
            title="确认删除此环境?"
            onConfirm={() => handleDelete(record.id, record.name)}
            okText="确认"
            cancelText="取消"
          >
            <Button size="small" danger icon={<DeleteOutlined />}>
              删除
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
            环境管理
          </Title>
        }
        extra={
          <Space>
            <Button type="primary" icon={<PlusOutlined />} onClick={handleCreate}>
              新建环境
            </Button>
            <Button icon={<ReloadOutlined />} onClick={() => refetch()}>
              刷新
            </Button>
          </Space>
        }
      >
        <Table
          columns={columns}
          dataSource={environments}
          rowKey="id"
          loading={isLoading}
          pagination={{
            pageSize: 10,
            showSizeChanger: true,
            showTotal: (total) => `共 ${total} 个环境`,
          }}
        />
      </Card>

      {/* Create/Edit Modal */}
      <Modal
        title={editingEnv ? '编辑环境' : '新建环境'}
        open={modalVisible}
        onOk={handleSubmit}
        onCancel={() => {
          setModalVisible(false);
          form.resetFields();
        }}
        confirmLoading={createMutation.isPending}
        destroyOnClose
      >
        <Form form={form} layout="vertical">
          <Form.Item
            name="name"
            label="环境名称"
            rules={[{ required: true, message: '请输入环境名称' }]}
          >
            <Input placeholder="例如: development, staging, production" />
          </Form.Item>
          <Form.Item name="description" label="描述">
            <TextArea rows={3} placeholder="环境描述" />
          </Form.Item>
          <Form.Item label="端点配置">
            <Form.List name="endpoints">
              {(fields, { add, remove }) => (
                <>
                  {fields.map(({ key, name, ...restField }) => (
                    <Space key={key} style={{ display: 'flex', marginBottom: 8 }} align="baseline">
                      <Form.Item
                        {...restField}
                        name={[name, 'key']}
                        rules={[{ required: true, message: '请输入端点名称' }]}
                      >
                        <Input placeholder="端点名称" />
                      </Form.Item>
                      <Form.Item
                        {...restField}
                        name={[name, 'value']}
                        rules={[{ required: true, message: '请输入端点 URL' }]}
                      >
                        <Input placeholder="端点 URL" />
                      </Form.Item>
                      <Button type="link" danger onClick={() => remove(name)}>
                        删除
                      </Button>
                    </Space>
                  ))}
                  <Button type="dashed" onClick={() => add()} block icon={<PlusOutlined />}>
                    添加端点
                  </Button>
                </>
              )}
            </Form.List>
          </Form.Item>
        </Form>
      </Modal>

      {/* Detail Modal */}
      <Modal
        title="环境详情"
        open={detailVisible}
        onCancel={() => {
          setDetailVisible(false);
          setSelectedEnv(null);
        }}
        footer={null}
        width={600}
      >
        {selectedEnv && (
          <Descriptions bordered column={1}>
            <Descriptions.Item label="环境名称">{selectedEnv.name}</Descriptions.Item>
            <Descriptions.Item label="描述">{selectedEnv.description || '-'}</Descriptions.Item>
            <Descriptions.Item label="类型">
              {selectedEnv.is_template ? '模板' : '环境'}
            </Descriptions.Item>
            <Descriptions.Item label="端点配置">
              {selectedEnv.endpoints && Object.keys(selectedEnv.endpoints).length > 0 ? (
                <Table
                  size="small"
                  dataSource={Object.entries(selectedEnv.endpoints).map(([key, value]) => ({
                    key,
                    name: key,
                    url: value,
                  }))}
                  columns={[
                    { title: '名称', dataIndex: 'name', key: 'name' },
                    { title: 'URL', dataIndex: 'url', key: 'url' },
                  ]}
                  pagination={false}
                />
              ) : (
                '无配置'
              )}
            </Descriptions.Item>
            <Descriptions.Item label="创建时间">
              {dayjs(selectedEnv.created_at).format('YYYY-MM-DD HH:mm:ss')}
            </Descriptions.Item>
            <Descriptions.Item label="更新时间">
              {dayjs(selectedEnv.updated_at).format('YYYY-MM-DD HH:mm:ss')}
            </Descriptions.Item>
          </Descriptions>
        )}
      </Modal>
    </div>
  );
};

export default EnvironmentsPage;
