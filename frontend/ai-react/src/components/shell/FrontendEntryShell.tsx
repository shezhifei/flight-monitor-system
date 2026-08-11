import type { PropsWithChildren } from 'react';
import { ConfigProvider, theme } from 'antd';

interface FrontendEntryShellProps extends PropsWithChildren {
  entryName: string;
  surface: 'page' | 'widget' | 'drawer' | 'modal';
}

export function FrontendEntryShell({ entryName, surface, children }: FrontendEntryShellProps) {
  return (
    <ConfigProvider
      theme={{
        algorithm: theme.darkAlgorithm,
        token: {
          colorPrimary: '#6366f1',
          colorBgContainer: '#161b22',
          colorBgElevated: '#1c2129',
          colorBgLayout: '#0e1117',
          colorBorder: 'rgba(255, 255, 255, 0.1)',
          colorBorderSecondary: 'rgba(255, 255, 255, 0.06)',
          colorText: '#e6edf3',
          colorTextSecondary: '#8b949e',
          colorTextTertiary: '#6e7681',
          colorTextQuaternary: '#484f58',
          borderRadius: 8,
          fontFamily: "'DM Sans', 'PingFang SC', 'Microsoft YaHei', sans-serif",
          fontFamilyCode: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
        },
        components: {
          Table: {
            colorBgContainer: '#161b22',
            headerBg: '#1c2129',
            headerColor: '#8b949e',
            rowHoverBg: 'rgba(255, 255, 255, 0.04)',
            borderColor: 'rgba(255, 255, 255, 0.08)',
            colorText: '#e6edf3',
          },
          Select: {
            colorBgContainer: '#161b22',
            colorBgElevated: '#1c2129',
            optionSelectedBg: 'rgba(99, 102, 241, 0.15)',
          },
          Input: {
            colorBgContainer: '#161b22',
          },
          InputNumber: {
            colorBgContainer: '#161b22',
          },
          Card: {
            colorBgContainer: 'rgba(255, 255, 255, 0.04)',
          },
          Drawer: {
            colorBgElevated: '#161b22',
          },
          Modal: {
            contentBg: '#161b22',
            headerBg: '#161b22',
          },
          Tabs: {
            inkBarColor: '#6366f1',
            itemActiveColor: '#6366f1',
            itemSelectedColor: '#6366f1',
          },
          Button: {
            primaryShadow: '0 0 12px rgba(99, 102, 241, 0.35)',
          },
          Alert: {
            colorInfoBg: 'rgba(99, 102, 241, 0.08)',
            colorInfoBorder: 'rgba(99, 102, 241, 0.2)',
          },
        },
      }}
    >
      <div
        className={`fm-entry-shell fm-entry-shell--${surface}`}
        data-fm-entry-shell={entryName}
        data-fm-entry-surface={surface}
      >
        {children}
      </div>
    </ConfigProvider>
  );
}
