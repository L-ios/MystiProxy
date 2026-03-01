import React, { useState } from 'react';
import {
  Card,
  Button,
  Space,
  Modal,
  Form,
  Select,
  Checkbox,
  Upload,
  message,
  Typography,
  Alert,
  Divider,
} from 'antd';
import {
  DownloadOutlined,
  UploadOutlined,
  FileTextOutlined,
  CheckCircleOutlined,
  CloseCircleOutlined,
} from '@ant-design/icons';
import type { UploadFile } from 'antd/es/upload/interface';
import { useExportConfig, useImportConfig, readFileContent } from '../../api/import-export';
import type { ExportRequest, ImportRequest, ImportResponse } from '../../types/api';

const { Text, Paragraph } = Typography;

const ImportExport: React.FC = () => {
  const [exportModalVisible, setExportModalVisible] = useState(false);
  const [importModalVisible, setImportModalVisible] = useState(false);
  const [exportForm] = Form.useForm();
  const [importForm] = Form.useForm();
  const [fileList, setFileList] = useState<UploadFile[]>([]);
  const [importResult, setImportResult] = useState<ImportResponse | null>(null);

  const exportMutation = useExportConfig();
  const importMutation = useImportConfig();

  const handleExport = async () => {
    try {
      const values = await exportForm.validateFields();
      const data: ExportRequest = {
        format: values.format,
        include_environments: values.include_environments,
        include_teams: values.include_teams,
      };

      await exportMutation.mutateAsync(data);
      message.success('配置导出成功');
      setExportModalVisible(false);
      exportForm.resetFields();
    } catch (error) {
      message.error(`导出失败: ${(error as Error).message}`);
    }
  };

  const handleImport = async () => {
    if (fileList.length === 0) {
      message.error('请选择要导入的文件');
      return;
    }

    try {
      const file = fileList[0].originFileObj;
      if (!file) {
        message.error('文件读取失败');
        return;
      }

      const content = await readFileContent(file);
      const values = await importForm.validateFields();

      const data: ImportRequest = {
        data: content,
        format: values.format,
        merge_strategy: values.merge_strategy,
      };

      const result = await importMutation.mutateAsync(data);
      setImportResult(result);

      if (result.error_count === 0) {
        message.success(`导入成功: ${result.imported_count} 个配置`);
      } else {
        message.warning(`导入完成: ${result.imported_count} 个成功, ${result.error_count} 个失败`);
      }
    } catch (error) {
      message.error(`导入失败: ${(error as Error).message}`);
    }
  };

  const handleFileChange = (info: { fileList: UploadFile[] }) => {
    setFileList(info.fileList.slice(-1));

    // Auto-detect format from file extension
    if (info.fileList.length > 0) {
      const file = info.fileList[0];
      if (file.name.endsWith('.yaml') || file.name.endsWith('.yml')) {
        importForm.setFieldValue('format', 'yaml');
      } else {
        importForm.setFieldValue('format', 'json');
      }
    }
  };

  const resetImportModal = () => {
    setImportModalVisible(false);
    setFileList([]);
    importForm.resetFields();
    setImportResult(null);
  };

  return (
    <Card title="导入导出">
      <Space direction="vertical" style={{ width: '100%' }}>
        <Paragraph type="secondary">
          导出当前系统的 Mock 配置，或从外部导入配置文件。
        </Paragraph>

        <Space>
          <Button
            type="primary"
            icon={<DownloadOutlined />}
            onClick={() => setExportModalVisible(true)}
          >
            导出配置
          </Button>
          <Button
            icon={<UploadOutlined />}
            onClick={() => setImportModalVisible(true)}
          >
            导入配置
          </Button>
        </Space>
      </Space>

      {/* Export Modal */}
      <Modal
        title="导出配置"
        open={exportModalVisible}
        onOk={handleExport}
        onCancel={() => {
          setExportModalVisible(false);
          exportForm.resetFields();
        }}
        confirmLoading={exportMutation.isPending}
      >
        <Form
          form={exportForm}
          layout="vertical"
          initialValues={{ format: 'json', include_environments: true, include_teams: true }}
        >
          <Form.Item
            name="format"
            label="导出格式"
            rules={[{ required: true, message: '请选择导出格式' }]}
          >
            <Select
              options={[
                { label: 'JSON', value: 'json' },
                { label: 'YAML', value: 'yaml' },
              ]}
            />
          </Form.Item>

          <Form.Item label="导出内容">
            <Form.Item name="include_environments" valuePropName="checked" noStyle>
              <Checkbox>包含环境配置</Checkbox>
            </Form.Item>
            <Form.Item name="include_teams" valuePropName="checked" noStyle>
              <Checkbox>包含团队配置</Checkbox>
            </Form.Item>
          </Form.Item>

          <Alert
            message="导出的配置文件将自动下载到本地"
            type="info"
            showIcon
          />
        </Form>
      </Modal>

      {/* Import Modal */}
      <Modal
        title="导入配置"
        open={importModalVisible}
        onOk={handleImport}
        onCancel={resetImportModal}
        confirmLoading={importMutation.isPending}
        width={600}
      >
        <Form
          form={importForm}
          layout="vertical"
          initialValues={{ format: 'json', merge_strategy: 'merge' }}
        >
          <Form.Item label="选择文件">
            <Upload
              fileList={fileList}
              beforeUpload={() => false}
              onChange={handleFileChange}
              accept=".json,.yaml,.yml"
              maxCount={1}
            >
              <Button icon={<FileTextOutlined />}>选择配置文件</Button>
            </Upload>
            <Text type="secondary" style={{ fontSize: 12 }}>
              支持 JSON 和 YAML 格式
            </Text>
          </Form.Item>

          <Form.Item
            name="format"
            label="文件格式"
            rules={[{ required: true, message: '请选择文件格式' }]}
          >
            <Select
              options={[
                { label: 'JSON', value: 'json' },
                { label: 'YAML', value: 'yaml' },
              ]}
            />
          </Form.Item>

          <Form.Item
            name="merge_strategy"
            label="合并策略"
            rules={[{ required: true, message: '请选择合并策略' }]}
          >
            <Select
              options={[
                { label: '合并 - 保留现有配置，添加新配置', value: 'merge' },
                { label: '替换 - 覆盖所有现有配置', value: 'replace' },
                { label: '跳过已存在 - 只导入新配置', value: 'skip_existing' },
              ]}
            />
          </Form.Item>

          <Alert
            message="导入操作可能会覆盖现有配置，请谨慎操作"
            type="warning"
            showIcon
          />

          {importResult && (
            <>
              <Divider />
              <Card size="small" title="导入结果">
                <Space direction="vertical" style={{ width: '100%' }}>
                  <Text>
                    <CheckCircleOutlined style={{ color: '#52c41a', marginRight: 8 }} />
                    成功导入: {importResult.imported_count} 个
                  </Text>
                  <Text>
                    <CloseCircleOutlined style={{ color: '#faad14', marginRight: 8 }} />
                    跳过: {importResult.skipped_count} 个
                  </Text>
                  {importResult.error_count > 0 && (
                    <>
                      <Text type="danger">
                        <CloseCircleOutlined style={{ marginRight: 8 }} />
                        失败: {importResult.error_count} 个
                      </Text>
                      {importResult.errors && importResult.errors.length > 0 && (
                        <div style={{ maxHeight: 150, overflow: 'auto' }}>
                          {importResult.errors.map((err, idx) => (
                            <Text key={idx} type="danger" style={{ display: 'block', fontSize: 12 }}>
                              {err.path}: {err.message}
                            </Text>
                          ))}
                        </div>
                      )}
                    </>
                  )}
                </Space>
              </Card>
            </>
          )}
        </Form>
      </Modal>
    </Card>
  );
};

export default ImportExport;
