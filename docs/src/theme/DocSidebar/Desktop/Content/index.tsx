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
    <div className="sidebar-version">
      <label htmlFor="sidebar-version-select">Version</label>
      <div className="sidebar-version__select">
        <select
          id="sidebar-version-select"
          value={activeVersion?.name || 'current'}
          onChange={(e) => {
            const selected = versions.find((v) => v.name === e.target.value);
            if (selected) {
              window.location.href = selected.path + '/';
            }
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
    </div>
  );
}

export default function ContentWrapper(props: Props): ReactNode {
  return (
    <>
      <VersionSelector />
      <Content {...props} />
      <div className="sidebar-ext">
        <a className="nw-more nw-more--accent" href="https://www.nobodywho.ai/" target="_blank" rel="noopener">
          nobodywho.ai
        </a>
        <a className="nw-more nw-more--accent" href="https://github.com/nobodywho-ooo/nobodywho" target="_blank" rel="noopener">
          GitHub
        </a>
      </div>
    </>
  );
}
