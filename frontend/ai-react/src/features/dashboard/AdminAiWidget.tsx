import { useEffect, useMemo, useRef, useState } from 'react';
import { Badge, Drawer, FloatButton, List, Space, Tag, Typography } from 'antd';
import { RobotOutlined } from '@ant-design/icons';
import { EventSourceClient } from '@/lib/sse/eventSourceClient';

interface EventItem {
  id: string;
  title: string;
  type: string;
  time: string;
}

export function AdminAiWidget(): JSX.Element {
  const [open, setOpen] = useState(false);
  const [events, setEvents] = useState<EventItem[]>([]);
  const [unreadCount, setUnreadCount] = useState(0);
  const openRef = useRef(false);

  useEffect(() => {
    openRef.current = open;
    if (open) {
      setUnreadCount(0);
    }
  }, [open]);

  useEffect(() => {
    const appendEvent = (item: EventItem): void => {
      setEvents((prev) => [...prev.slice(-99), item]);
      if (!openRef.current) {
        setUnreadCount((count) => count + 1);
      }
    };
    const client = new EventSourceClient({
      url: '/api/v2/ai/events/stream',
      eventNames: ['ai_execution'],
      clientScope: 'dashboard_admin_ai',
      onEvent: (eventName, payload) => {
        const runtime = payload.payload && typeof payload.payload === 'object'
          ? (payload.payload as Record<string, unknown>)
          : payload;
        const runtimeRecord = runtime && typeof runtime === 'object'
          ? runtime as Record<string, unknown>
          : {};
        const semantic = String(runtimeRecord.event || payload.type || eventName || '').toLowerCase();
        if (!['approval_required', 'approval_result', 'tool_start', 'tool_end', 'execution_end'].includes(semantic)) {
          return;
        }
        const id = `${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        const title = String(runtimeRecord.message || runtimeRecord.tool_name || semantic || 'ai_event');
        appendEvent({ id, title, type: semantic, time: new Date().toLocaleTimeString() });
      },
      onError: () => {
        appendEvent({
          id: `err_${Date.now()}`,
          title: 'AI 实时事件连接已断开，等待自动重连',
          type: 'stream_error',
          time: new Date().toLocaleTimeString(),
        });
      },
    });
    client.connect();
    return () => client.disconnect();
  }, []);

  const unread = useMemo(() => unreadCount, [unreadCount]);

  return (
    <>
      <Badge count={unread > 99 ? '99+' : unread} offset={[-6, 6]}>
        <FloatButton
          icon={<RobotOutlined />}
          type="primary"
          style={{ right: 28, bottom: 28 }}
          tooltip="管理员 AI 事件"
          onClick={() => setOpen(true)}
        />
      </Badge>
      <Drawer
        title="管理员 AI 助手"
        open={open}
        width={420}
        onClose={() => setOpen(false)}
        extra={
          <Space>
            <Tag color="blue">事件 {events.length}</Tag>
          </Space>
        }
      >
        <List
          dataSource={[...events].reverse()}
          renderItem={(item) => (
            <List.Item>
              <List.Item.Meta
                title={
                  <Space>
                    <Typography.Text>{item.title}</Typography.Text>
                    <Tag>{item.type}</Tag>
                  </Space>
                }
                description={item.time}
              />
            </List.Item>
          )}
        />
      </Drawer>
    </>
  );
}
