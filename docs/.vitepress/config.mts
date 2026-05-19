import { defineConfig } from 'vitepress'

const base = process.env.DOCS_BASE ?? '/'

export default defineConfig({
  title: 'Action Gateway',
  description: 'Action Gateway user documentation',
  lang: 'zh-CN',
  base,
  cleanUrls: true,
  lastUpdated: true,
  markdown: {
    lineNumbers: true
  },
  themeConfig: {
    logo: '/logo.svg',
    nav: [
      { text: '开始使用', link: '/guide/getting-started' },
      { text: '配置', link: '/guide/configure-sources' },
      { text: '接入 Codex', link: '/guide/mcp-client' },
      { text: '参考', link: '/reference/tools' }
    ],
    sidebar: [
      {
        text: '开始使用',
        items: [
          { text: '快速开始', link: '/guide/getting-started' },
          { text: '核心概念', link: '/guide/concepts' },
          { text: '配置 Source 和 Allowlist', link: '/guide/configure-sources' },
          { text: '使用 agctl 管理权限', link: '/guide/agctl' },
          { text: '接入 Codex', link: '/guide/mcp-client' },
          { text: '部署与运维', link: '/guide/deployment' },
          { text: '故障排查', link: '/guide/troubleshooting' }
        ]
      },
      {
        text: '参考',
        items: [
          { text: 'MCP Tools', link: '/reference/tools' },
          { text: '文件存储结构', link: '/reference/store' },
          { text: 'agctl YAML', link: '/reference/agctl-yaml' },
          { text: 'Admin JSON API', link: '/reference/admin-api' }
        ]
      }
    ],
    search: {
      provider: 'local'
    },
    outline: {
      label: '本页目录',
      level: [2, 3]
    },
    lastUpdated: {
      text: '最后更新'
    },
    docFooter: {
      prev: '上一页',
      next: '下一页'
    },
    footer: {
      message: 'Action Gateway user documentation',
      copyright: 'Built with VitePress'
    }
  }
})
