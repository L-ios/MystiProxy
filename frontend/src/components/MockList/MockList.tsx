import React, { useState } from 'react';
import {
  Table,
  Button,
  Space,
  Tag,
  message,
  Popconfirm,
  Input,
  Select,
  Card,
} from 'antd';
import {
  PlusOutlined,
  DeleteOutlined,
  EditOutlined,
  SearchOutlined,
  ReloadOutlined,
} from '@ant-design/icons';
import { useNavigate } from 'react-router-dom';
import { useMocks, useDeleteMock, useBatchDeleteMocks } from '../../api/mocks';
import type { MockConfiguration, MockFilter } from '../../types/api';
import type { ColumnsType, TablePaginationConfig } from 'antd/es/table';

const { Option } = Select;

const MockList: React.FC = () => {
  const navigate = useNavigate();
  const [filter, setFilter] = useState<MockFilter>({
    page: 1,
    limit: 20,
  });
  const [selectedRowKeys, setSelectedRowKeys] = useState<React.Key[]>([]);
  const [searchText, setSearchText] = useState('');

  const { data, isLoading, refetch } = useMocks(filter);
  const deleteMock = useDeleteMock();
  const batchDeleteMocks = useBatchDeleteMocks();

  const handleTableChange = (pagination: TablePaginationConfig) => {
    setFilter({
      ...filter,
      page: pagination.current,
      limit: pagination.pageSize,
    });
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteMock.mutateAsync(id);
      message.success('删除成功');
    } catch (error) {
      message.error('删除失败');
    }
  };

  const handleBatchDelete = async () => {
    if (selectedRowKeys.length === 0) {
      message.warning('请选择要删除的项');
      return;
    }

    try {
      await batchDeleteMocks.mutateAsync(selectedRowKeys as string[]);
      message.success('批量删除成功');
      setSelectedRowKeys([]);
    } catch (error) {
      message.error('批量删除失败');
    }
  };

  const handleSearch = () => {
    setFilter({
      ...filter,
      path: searchText,
      page: 1,
    });
  };

  const getMethodColor = (method: string) => {
    const colors: Record<string, string> = {
      GET: 'green',
      POST: 'blue',
      PUT: 'orange',
      DELETE: 'red',
      PATCH: 'purple',
    };
    return colors[method] || 'default';
  };

  const columns: ColumnsType<MockConfiguration> = [
    {
      title: '名称',
      dataIndex: 'name',
      key: 'name',
      width: 200,
      ellipsis: true,
    },
    {
      title: '路径',
      dataIndex: 'path',
      key: 'path',
      width: 250,
      ellipsis: true,
    },
    {
      title: '方法',
      dataIndex: 'method',
      key: 'method',
      width: 100,
      render: (method: string) => (
        <Tag color={getMethodColor(method)}>{method}</Tag>
      ),
    },
    {
      title: '状态码',
      dataIndex: ['response_config', 'status'],
      key: 'status',
      width: 100,
    },
    {
      title: '来源',
      dataIndex: 'source',
      key: 'source',
      width: 100,
      render: (source: string) => (
        <Tag color={source === 'central' ? 'blue' : 'green'}>
          {source === 'central' ? '中心' : '本地'}
        </Tag>
      ),
    },
    {
      title: '创建时间',
      dataIndex: 'created_at',
      key: 'created_at',
      width: 180,
      render: (date: string) => new Date(date).toLocaleString('zh-CN'),
    },
    {
      title: '操作',
      key: 'action',
      width: 150,
      fixed: 'right',
      render: (_, record) => (
        <Space size="small">
          <Button
            type="link"
            size="small"
            icon={<EditOutlined />}
            onClick={() => navigate(`/mocks/edit/${record.id}`)}
          >
            编辑
          </Button>
          <Popconfirm
            title="确定要删除这个 Mock 吗？"
            onConfirm={() => handleDelete(record.id)}
            okText="确定"
            cancelText="取消"
          >
            <Button type="link" size="small" danger icon={<DeleteOutlined />}>
              删除
            </Button>
          </Popconfirm>
        </Space>
      ),
    },
  ];

  const rowSelection = {
    selectedRowKeys,
    onChange: (newSelectedRowKeys: React.Key[]) => {
      setSelectedRowKeys(newSelectedRowKeys);
    },
  };

  return (
    <div>
      <Card>
        <Space direction="vertical" size="large" style={{ width: '100%' }}>
          <Space style={{ width: '100%', justifyContent: 'space-between' }}>
            <Space>
              <Input
                placeholder="搜索路径"
                value={searchText}
                onChange={(e) => setSearchText(e.target.value)}
                onPressEnter={handleSearch}
                style={{ width: 250 }}
                prefix={<SearchOutlined />}
              />
              <Select
                placeholder="选择方法"
                style={{ width: 120 }}
                allowClear
                onChange={(value) => setFilter({ ...filter, method: value })}
              >
                <Option value="GET">GET</Option>
                <Option value="POST">POST</Option>
                <Option value="PUT">PUT</Option>
                <Option value="DELETE">DELETE</Option>
                <Option value="PATCH">PATCH</Option>
              </Select>
              <Button icon={<SearchOutlined />} onClick={handleSearch}>
                搜索
              </Button>
            </Space>
            <Space>
              <Button
                danger
                icon={<DeleteOutlined />}
                onClick={handleBatchDelete}
                disabled={selectedRowKeys.length === 0}
              >
                批量删除
              </Button>
              <Button
                icon={<ReloadOutlined />}
                onClick={() => refetch()}
              >
                刷新
              </Button>
              <Button
                type="primary"
                icon={<PlusOutlined />}
                onClick={() => navigate('/mocks/create')}
              >
                新建 Mock
              </Button>
            </Space>
          </Space>

          <Table
            columns={columns}
            dataSource={data?.data || []}
            rowKey="id"
            loading={isLoading}
            rowSelection={rowSelection}
            pagination={{
              current: filter.page,
              pageSize: filter.limit,
              total: data?.pagination.total || 0,
              showSizeChanger: true,
              showQuickJumper: true,
              showTotal: (total) => `共 ${total} 条`,
            }}
            onChange={handleTableChange}
            scroll={{ x: 1200 }}
          />
        </Space>
      </Card>
    </div>
  );
};

export default MockList;
