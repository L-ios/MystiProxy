import React, { useState } from 'react';
import { Card, Row, Col, Statistic, DatePicker, Space, Select, Spin, Empty } from 'antd';
import {
  ApiOutlined,
  ClockCircleOutlined,
  CloseCircleOutlined,
  CheckCircleOutlined,
} from '@ant-design/icons';
import ReactECharts from 'echarts-for-react';
import { useAnalytics } from '../../api/analytics';
import { useMocks } from '../../api/mocks';
import { useEnvironments } from '../../api/environments';
import dayjs from 'dayjs';

const { RangePicker } = DatePicker;

const AnalyticsPage: React.FC = () => {
  const [dateRange, setDateRange] = useState<[dayjs.Dayjs, dayjs.Dayjs] | null>(null);
  const [selectedMock, setSelectedMock] = useState<string | undefined>();
  const [selectedEnv, setSelectedEnv] = useState<string | undefined>();

  const { data: analyticsData, isLoading } = useAnalytics({
    start_date: dateRange?.[0]?.format('YYYY-MM-DD'),
    end_date: dateRange?.[1]?.format('YYYY-MM-DD'),
    mock_id: selectedMock,
    environment: selectedEnv,
  });

  const { data: mocksData } = useMocks({ limit: 100 });
  const { data: envsData } = useEnvironments();

  const mocks = mocksData?.data || [];
  const environments = envsData?.data || [];

  const getRequestChartOption = () => {
    if (!analyticsData?.request_stats) return {};

    const dates = analyticsData.request_stats.map((item) => item.date);
    const successCounts = analyticsData.request_stats.map((item) => item.success_count);
    const errorCounts = analyticsData.request_stats.map((item) => item.error_count);

    return {
      title: {
        text: '请求统计',
        left: 'center',
      },
      tooltip: {
        trigger: 'axis',
        axisPointer: {
          type: 'shadow',
        },
      },
      legend: {
        data: ['成功请求', '错误请求'],
        bottom: 0,
      },
      grid: {
        left: '3%',
        right: '4%',
        bottom: '15%',
        containLabel: true,
      },
      xAxis: {
        type: 'category',
        data: dates,
      },
      yAxis: {
        type: 'value',
      },
      series: [
        {
          name: '成功请求',
          type: 'bar',
          stack: 'total',
          data: successCounts,
          itemStyle: { color: '#52c41a' },
        },
        {
          name: '错误请求',
          type: 'bar',
          stack: 'total',
          data: errorCounts,
          itemStyle: { color: '#ff4d4f' },
        },
      ],
    };
  };

  const getResponseTimeChartOption = () => {
    if (!analyticsData?.response_time_stats) return {};

    const dates = analyticsData.response_time_stats.map((item) => item.date);
    const avgTimes = analyticsData.response_time_stats.map((item) => item.avg_time);
    const p95Times = analyticsData.response_time_stats.map((item) => item.p95);
    const p99Times = analyticsData.response_time_stats.map((item) => item.p99);

    return {
      title: {
        text: '响应时间趋势',
        left: 'center',
      },
      tooltip: {
        trigger: 'axis',
      },
      legend: {
        data: ['平均响应时间', 'P95', 'P99'],
        bottom: 0,
      },
      grid: {
        left: '3%',
        right: '4%',
        bottom: '15%',
        containLabel: true,
      },
      xAxis: {
        type: 'category',
        boundaryGap: false,
        data: dates,
      },
      yAxis: {
        type: 'value',
        name: 'ms',
      },
      series: [
        {
          name: '平均响应时间',
          type: 'line',
          data: avgTimes,
          smooth: true,
          itemStyle: { color: '#1890ff' },
        },
        {
          name: 'P95',
          type: 'line',
          data: p95Times,
          smooth: true,
          itemStyle: { color: '#faad14' },
        },
        {
          name: 'P99',
          type: 'line',
          data: p99Times,
          smooth: true,
          itemStyle: { color: '#ff4d4f' },
        },
      ],
    };
  };

  const getTopMocksChartOption = () => {
    if (!analyticsData?.top_mocks) return {};

    const data = analyticsData.top_mocks.slice(0, 10).map((item) => ({
      name: item.mock_name,
      value: item.request_count,
    }));

    return {
      title: {
        text: '热门 Mock Top 10',
        left: 'center',
      },
      tooltip: {
        trigger: 'item',
        formatter: '{b}: {c} ({d}%)',
      },
      series: [
        {
          type: 'pie',
          radius: ['40%', '70%'],
          avoidLabelOverlap: false,
          itemStyle: {
            borderRadius: 10,
            borderColor: '#fff',
            borderWidth: 2,
          },
          label: {
            show: false,
            position: 'center',
          },
          emphasis: {
            label: {
              show: true,
              fontSize: 16,
              fontWeight: 'bold',
            },
          },
          labelLine: {
            show: false,
          },
          data,
        },
      ],
    };
  };

  const getErrorRateChartOption = () => {
    if (!analyticsData?.request_stats) return {};

    const dates = analyticsData.request_stats.map((item) => item.date);
    const errorRates = analyticsData.request_stats.map((item) => {
      const total = item.success_count + item.error_count;
      return total > 0 ? ((item.error_count / total) * 100).toFixed(2) : 0;
    });

    return {
      title: {
        text: '错误率趋势',
        left: 'center',
      },
      tooltip: {
        trigger: 'axis',
        formatter: '{b}<br />{a}: {c}%',
      },
      grid: {
        left: '3%',
        right: '4%',
        bottom: '3%',
        containLabel: true,
      },
      xAxis: {
        type: 'category',
        boundaryGap: false,
        data: dates,
      },
      yAxis: {
        type: 'value',
        name: '%',
        max: 100,
      },
      series: [
        {
          name: '错误率',
          type: 'line',
          data: errorRates,
          smooth: true,
          areaStyle: {
            color: {
              type: 'linear',
              x: 0,
              y: 0,
              x2: 0,
              y2: 1,
              colorStops: [
                { offset: 0, color: 'rgba(255, 77, 79, 0.3)' },
                { offset: 1, color: 'rgba(255, 77, 79, 0.05)' },
              ],
            },
          },
          itemStyle: { color: '#ff4d4f' },
        },
      ],
    };
  };

  return (
    <div>
      <Card style={{ marginBottom: 16 }}>
        <Space wrap>
          <RangePicker
            onChange={(dates) => {
              if (dates && dates[0] && dates[1]) {
                setDateRange([dates[0], dates[1]]);
              } else {
                setDateRange(null);
              }
            }}
          />
          <Select
            allowClear
            style={{ width: 200 }}
            placeholder="选择 Mock"
            onChange={setSelectedMock}
            options={mocks.map((mock) => ({
              label: mock.name,
              value: mock.id,
            }))}
          />
          <Select
            allowClear
            style={{ width: 200 }}
            placeholder="选择环境"
            onChange={setSelectedEnv}
            options={environments.map((env) => ({
              label: env.name,
              value: env.id,
            }))}
          />
        </Space>
      </Card>

      {isLoading ? (
        <Card>
          <Spin size="large" style={{ display: 'block', margin: '100px auto' }} />
        </Card>
      ) : !analyticsData ? (
        <Card>
          <Empty description="暂无数据" />
        </Card>
      ) : (
        <>
          <Row gutter={[16, 16]}>
            <Col xs={24} sm={12} lg={6}>
              <Card>
                <Statistic
                  title="总请求数"
                  value={analyticsData.overview.total_requests}
                  prefix={<ApiOutlined />}
                  valueStyle={{ color: '#1890ff' }}
                />
              </Card>
            </Col>
            <Col xs={24} sm={12} lg={6}>
              <Card>
                <Statistic
                  title="平均响应时间"
                  value={analyticsData.overview.avg_response_time}
                  suffix="ms"
                  prefix={<ClockCircleOutlined />}
                  valueStyle={{ color: '#52c41a' }}
                />
              </Card>
            </Col>
            <Col xs={24} sm={12} lg={6}>
              <Card>
                <Statistic
                  title="错误率"
                  value={analyticsData.overview.error_rate}
                  suffix="%"
                  prefix={<CloseCircleOutlined />}
                  valueStyle={{ color: analyticsData.overview.error_rate > 5 ? '#ff4d4f' : '#52c41a' }}
                />
              </Card>
            </Col>
            <Col xs={24} sm={12} lg={6}>
              <Card>
                <Statistic
                  title="活跃 Mock"
                  value={analyticsData.overview.active_mocks}
                  prefix={<CheckCircleOutlined />}
                  valueStyle={{ color: '#722ed1' }}
                />
              </Card>
            </Col>
          </Row>

          <Row gutter={[16, 16]} style={{ marginTop: 16 }}>
            <Col xs={24} lg={12}>
              <Card>
                <ReactECharts option={getRequestChartOption()} style={{ height: 350 }} />
              </Card>
            </Col>
            <Col xs={24} lg={12}>
              <Card>
                <ReactECharts option={getResponseTimeChartOption()} style={{ height: 350 }} />
              </Card>
            </Col>
          </Row>

          <Row gutter={[16, 16]} style={{ marginTop: 16 }}>
            <Col xs={24} lg={12}>
              <Card>
                <ReactECharts option={getErrorRateChartOption()} style={{ height: 350 }} />
              </Card>
            </Col>
            <Col xs={24} lg={12}>
              <Card>
                <ReactECharts option={getTopMocksChartOption()} style={{ height: 350 }} />
              </Card>
            </Col>
          </Row>

          <Card title="热门 Mock 详情" style={{ marginTop: 16 }}>
            <table style={{ width: '100%', borderCollapse: 'collapse' }}>
              <thead>
                <tr style={{ borderBottom: '1px solid #f0f0f0' }}>
                  <th style={{ padding: '12px 8px', textAlign: 'left' }}>Mock 名称</th>
                  <th style={{ padding: '12px 8px', textAlign: 'right' }}>请求数</th>
                  <th style={{ padding: '12px 8px', textAlign: 'right' }}>平均响应时间</th>
                  <th style={{ padding: '12px 8px', textAlign: 'right' }}>错误数</th>
                </tr>
              </thead>
              <tbody>
                {analyticsData.top_mocks.map((mock, index) => (
                  <tr key={mock.mock_id} style={{ borderBottom: '1px solid #f0f0f0' }}>
                    <td style={{ padding: '12px 8px' }}>
                      {index + 1}. {mock.mock_name}
                    </td>
                    <td style={{ padding: '12px 8px', textAlign: 'right' }}>{mock.request_count}</td>
                    <td style={{ padding: '12px 8px', textAlign: 'right' }}>{mock.avg_response_time}ms</td>
                    <td style={{ padding: '12px 8px', textAlign: 'right', color: mock.error_count > 0 ? '#ff4d4f' : undefined }}>
                      {mock.error_count}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </Card>
        </>
      )}
    </div>
  );
};

export default AnalyticsPage;
