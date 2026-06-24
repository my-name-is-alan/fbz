import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { describe, it } from 'node:test'
import { fileURLToPath } from 'node:url'

const examplesRoot = dirname(fileURLToPath(import.meta.url))

const templates = [
  {
    dir: 'telegram-notifier-template',
    id: 'dev.fbz.notify.telegram',
    name: 'Telegram Notification Template',
    port: 19093,
    defaultChannel: 'telegram'
  },
  {
    dir: 'wecom-notifier-template',
    id: 'dev.fbz.notify.wecom',
    name: 'WeCom Notification Template',
    port: 19094,
    defaultChannel: 'wecom'
  },
  {
    dir: 'webhook-notifier-template',
    id: 'dev.fbz.notify.webhook',
    name: 'Webhook Notification Template',
    port: 19095,
    defaultChannel: 'webhook'
  }
]

const expectedHooks = [
  'library.scan.completed',
  'library.scan.failed',
  'metadata.refresh.failed',
  'transcode.failed',
  'user.login'
]

describe('first-party notification plugin templates', () => {
  for (const template of templates) {
    it(`${template.dir} keeps notification delivery administrator managed`, async () => {
      const pluginDir = join(examplesRoot, template.dir)
      const manifest = JSON.parse(await readFile(join(pluginDir, 'manifest.json'), 'utf8'))
      const server = await readFile(join(pluginDir, 'server.mjs'), 'utf8')
      const readme = await readFile(join(pluginDir, 'README.md'), 'utf8')

      assert.equal(manifest.id, template.id)
      assert.equal(manifest.name, template.name)
      assert.equal(manifest.runtime, 'http')
      assert.equal(manifest.apiVersion, '1')
      assert.equal(manifest.entrypoint, `http://127.0.0.1:${template.port}/fbz-plugin`)
      assert.deepEqual(
        manifest.permissions.map((permission) => permission.key),
        ['notification.send']
      )

      const hooks = manifest.hooks.map((hook) => hook.event)
      for (const hook of expectedHooks) {
        assert.ok(hooks.includes(hook), `${template.dir} should subscribe to ${hook}`)
      }

      const configKeys = manifest.configSchema.map((field) => field.key)
      assert.ok(configKeys.includes('channel'))
      assert.ok(configKeys.includes('title_prefix'))
      assert.ok(configKeys.includes('include_payload'))
      assert.ok(manifest.description.includes('administrator-managed notification targets'))
      assert.ok(manifest.description.includes(template.defaultChannel))

      for (const field of manifest.configSchema) {
        assert.notEqual(field.type, 'secret')
        assert.notEqual(field.type, 'password')
      }

      assert.match(server, /hostJson\(context, '\/api\/plugin\/notifications'/)
      assert.match(server, /loadPluginConfig\(context/)
      assert.match(server, /createHttpPluginServer/)
      assert.match(server, new RegExp(`const defaultChannel = '${template.defaultChannel}'`))
      assert.doesNotMatch(server, /api\.telegram\.org|qyapi\.weixin|webhookUrl|botToken/)

      assert.match(readme, /POST \/api\/plugin\/notifications/)
      assert.match(readme, /administrator-managed notification targets/)
      assert.doesNotMatch(readme, /paste.*bot token|paste.*webhook URL/i)
    })
  }
})
