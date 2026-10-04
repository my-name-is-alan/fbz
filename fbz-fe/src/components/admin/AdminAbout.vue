<script setup lang="ts">
import { getSystemInfo } from "@/service/modules/admin.ts";
import type { SystemInfo } from "@/types/admin.ts";

const info = ref<SystemInfo | null>(null);
const loading = ref(false);
const loadError = ref("");

const techStack = computed(() => {
  const i = info.value;
  if (!i) return [];
  const rows: { name: string; val: string }[] = [
    { name: "版本", val: i.version },
    { name: "构建配置", val: i.buildProfile },
    { name: "运行平台", val: i.os },
  ];
  if (i.rustVersion) rows.push({ name: "Rust", val: i.rustVersion });
  return rows;
});

onMounted(() => {
  void loadInfo();
});

async function loadInfo() {
  loading.value = true;
  loadError.value = "";
  try {
    info.value = await getSystemInfo();
  } catch {
    loadError.value = "系统信息加载失败，请确认后端已就绪且当前账号具备管理员权限。";
  } finally {
    loading.value = false;
  }
}
</script>

<template>
  <div class="admin-about-view">
    <div class="about-card">
      <div class="brand">
        <span class="logo">F<b>B</b>Z</span>
        <span class="version">Server {{ info?.version ?? "…" }}</span>
      </div>

      <p v-if="loadError" class="load-error">{{ loadError }}</p>

      <div v-if="info" class="tech-stack-section">
        <h4>系统运行环境</h4>
        <div class="tech-grid">
          <div v-for="row in techStack" :key="row.name" class="tech-item">
            <span class="tech-name">{{ row.name }}</span>
            <span class="tech-val">{{ row.val }}</span>
          </div>
        </div>
      </div>

      <div v-if="info" class="tech-stack-section">
        <h4>连接与规模</h4>
        <div class="tech-grid">
          <div class="tech-item">
            <span class="tech-name">数据库</span>
            <span class="tech-val" :class="info.databaseConnected ? 'ok' : 'down'">
              {{ info.databaseConnected ? "已连接" : "未连接" }}
            </span>
          </div>
          <div class="tech-item">
            <span class="tech-name">Redis</span>
            <span class="tech-val" :class="info.redisConnected ? 'ok' : 'down'">
              {{ info.redisConnected ? "已连接" : "未连接" }}
            </span>
          </div>
          <div class="tech-item">
            <span class="tech-name">媒体库</span>
            <span class="tech-val">{{ info.libraryCount }}</span>
          </div>
          <div class="tech-item">
            <span class="tech-name">用户</span>
            <span class="tech-val">{{ info.userCount }}</span>
          </div>
          <div class="tech-item">
            <span class="tech-name">媒体条目</span>
            <span class="tech-val">{{ info.mediaItemCount }}</span>
          </div>
        </div>
      </div>

      <div class="about-info-text">
        <p>
          fbz
          是一款全功能、轻量化自托管家庭网络视频与媒体数据库管理系统。支持影片元数据智能搜刮过滤、多设备客户端串流播放、多版本视频文件加载及服务端硬件解码转码功能。
        </p>
        <p class="copyright">Copyright &copy; 2026 FBZ System. MIT Licensed.</p>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
.admin-about-view {
  max-width: 600px;
}

.about-card {
  background: var(--fbz-color-panel-strong);
  border: 1px solid var(--fbz-color-line-soft);
  border-radius: 6px;
  padding: 32px;
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.brand {
  display: flex;
  flex-direction: column;
  gap: 6px;
  align-items: center;
  text-align: center;

  .logo {
    font-family: var(--fbz-font-display);
    font-weight: 800;
    font-size: 36px;
    letter-spacing: 4px;
    color: var(--fbz-color-text);

    b {
      color: var(--fbz-color-brand-500);
    }
  }

  .version {
    font-size: var(--fbz-font-size-sm);
    color: var(--fbz-color-text-muted);
    font-weight: 700;
    letter-spacing: 1px;
  }
}

.tech-stack-section {
  border-top: 1px solid var(--fbz-color-line-soft);
  padding-top: 20px;

  h4 {
    margin: 0 0 12px;
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--fbz-color-text-muted);
    letter-spacing: 0.5px;
  }
}

.tech-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 12px;
}

.tech-item {
  display: flex;
  justify-content: space-between;
  background: var(--fbz-color-panel);
  border: 1px solid var(--fbz-color-line);
  padding: 8px 14px;
  border-radius: 4px;
  font-size: var(--fbz-font-size-xs);

  .tech-name {
    font-weight: 700;
    color: var(--fbz-color-text-soft);
  }

  .tech-val {
    color: var(--fbz-color-text-muted);
    font-family: var(--fbz-font-display);

    &.ok {
      color: var(--fbz-color-brand-500);
    }

    &.down {
      color: var(--fbz-color-danger-500);
    }
  }
}

.load-error {
  margin: 0;
  color: var(--fbz-color-danger-500);
  font-size: var(--fbz-font-size-sm);
}

.about-info-text {
  border-top: 1px solid var(--fbz-color-line-soft);
  padding-top: 20px;

  p {
    margin: 0 0 10px;
    font-size: 13px;
    color: var(--fbz-color-text-soft);
    line-height: 1.6;

    &:last-child {
      margin-bottom: 0;
    }
  }

  .copyright {
    font-size: 11px;
    color: var(--fbz-color-text-muted);
    margin-top: 14px;
  }
}
</style>
