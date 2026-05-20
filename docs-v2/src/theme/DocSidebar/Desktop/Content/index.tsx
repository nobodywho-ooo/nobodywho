import React, {type ReactNode} from 'react';
import Content from '@theme-original/DocSidebar/Desktop/Content';
import type ContentType from '@theme/DocSidebar/Desktop/Content';
import type {WrapperProps} from '@docusaurus/types';
import {
  useActivePlugin,
  useVersions,
  useActiveVersion,
} from '@docusaurus/plugin-content-docs/client';
import Link from '@docusaurus/Link';

type Props = WrapperProps<typeof ContentType>;

function VersionSelector() {
  const plugin = useActivePlugin();
  if (!plugin) return null;

  const {pluginId} = plugin;
  // Don't show for the default/shared docs
  if (pluginId === 'default') return null;

  const versions = useVersions(pluginId);
  const activeVersion = useActiveVersion(pluginId);

  // Only show if there are multiple versions
  if (versions.length <= 1) return null;

  return (
    <div style={{
      padding: '0.5rem 0.75rem 0.75rem',
      borderBottom: '1px solid var(--ifm-toc-border-color)',
    }}>
      <div style={{
        fontSize: '0.7rem',
        fontWeight: 500,
        textTransform: 'uppercase',
        letterSpacing: '0.05em',
        color: 'var(--ifm-font-color-secondary)',
        marginBottom: '0.35rem',
      }}>
        Version
      </div>
      <select
        value={activeVersion?.name || 'current'}
        onChange={(e) => {
          const selected = versions.find((v) => v.name === e.target.value);
          if (selected) {
            window.location.href = selected.path + '/';
          }
        }}
        style={{
          width: '100%',
          padding: '0.35rem 0.5rem',
          fontSize: '0.85rem',
          fontWeight: 400,
          fontFamily: 'var(--ifm-font-family-base)',
          borderRadius: '6px',
          border: '1px solid var(--ifm-toc-border-color)',
          backgroundColor: 'var(--ifm-background-surface-color)',
          color: '#fff',
          cursor: 'pointer',
          appearance: 'auto',
        }}
        className="sidebar-version-select"
      >
        {versions.map((v) => (
          <option key={v.name} value={v.name}>
            {v.label}
          </option>
        ))}
      </select>
    </div>
  );
}

export default function ContentWrapper(props: Props): ReactNode {
  return (
    <>
      <VersionSelector />
      <Content {...props} />
    </>
  );
}
