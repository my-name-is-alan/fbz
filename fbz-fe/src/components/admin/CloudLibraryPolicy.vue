<script setup lang="ts">
const nfoSource = defineModel<string>("nfoSource", { required: true });
const imageCache = defineModel<string>("imageCache", { required: true });
const refreshMinutes = defineModel<number>("refreshMinutes", { required: true });
</script>
<template>
  <div class="policy-fields">
    <label
      >NFO 来源<BaseSelect
        v-model="nfoSource"
        :options="[
          { label: '云端已有 NFO（缺失时按文件名识别）', value: 'cloud' },
          { label: '仅文件名，不读取 NFO', value: 'filename' },
        ]"
    /></label>
    <p>读取挂载目录中的资料，不调用在线刮削器、不改写云端文件。</p>
    <label
      >图片缓存<BaseSelect
        v-model="imageCache"
        :options="[
          { label: '按需缓存 · 浏览图片时读取', value: 'on_demand' },
          { label: '扫描时预缓存 · 入库后快速显示', value: 'prefetch' },
        ]"
    /></label>
    <label
      >自动刷新周期（分钟）<input
        class="du-input"
        type="number"
        v-model.number="refreshMinutes"
        min="0"
        max="10080"
        required
    /></label>
    <p>0 表示手动；自动刷新最短 5 分钟。服务端定时扫描，关闭网页仍会执行；失败退避并续扫。</p>
  </div>
</template>
<style scoped>
.policy-fields {
  display: grid;
  gap: 16px;
}
.policy-fields label {
  display: grid;
  gap: 9px;
  font-size: 13px;
}
.policy-fields p {
  font-size: 12px;
  line-height: 1.7;
  color: var(--fbz-color-text-muted);
}
.du-input {
  width: 100%;
}
</style>
