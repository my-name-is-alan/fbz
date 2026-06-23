const asset = (name) => `./assets/${name}`;

const navItems = [
  { id: "home", label: "首页", title: "首页", eyebrow: "Watch Home", copy: "继续观看、发现新片、进入你的家庭媒体库" },
  { id: "detail", label: "影视详情", title: "影视详情", eyebrow: "Media Detail", copy: "播放、收藏、剧集进度与技术信息" },
  { id: "admin", label: "管理后台", title: "NAS 管理后台", eyebrow: "Admin", copy: "媒体扫描、存储、用户与转码任务" },
  { id: "system", label: "组件规范", title: "组件与设计规范", eyebrow: "Design System", copy: "Material 3 / Simple Design System 风格迁移说明" },
];

const continueItems = [
  { title: "北境余烬 S4 E3", meta: "剧集 · 52 min · 已观看 68%", image: "episode-3.png", progress: 68 },
  { title: "群山之影 S2 E6", meta: "剧集 · 47 min · 已观看 31%", image: "episode-1.png", progress: 31 },
  { title: "夜航档案 S1 E2", meta: "纪录片 · 44 min · 已观看 44%", image: "episode-2.png", progress: 44 },
];

const movies = [
  { title: "沙丘：觉醒", meta: "电影 · 4K · HDR", image: "rec-1.png", rating: "8.7" },
  { title: "孤城档案", meta: "电影 · 1080p", image: "rec-2.png", rating: "9.1" },
  { title: "维京征途", meta: "电影 · 4K", image: "rec-3.png", rating: "8.5" },
  { title: "最后王国", meta: "电影 · 4K", image: "rec-4.png", rating: "8.3" },
  { title: "暗骨王冠", meta: "电影 · 1080p", image: "rec-5.png", rating: "7.9" },
  { title: "深空边界", meta: "电影 · 4K", image: "rec-6.png", rating: "8.1" },
];

const categories = [
  ["电影", "1,284 部"],
  ["剧集", "342 部"],
  ["动画", "186 部"],
  ["纪录片", "98 部"],
  ["家庭收藏", "76 个"],
];

const episodes = [
  ["双剑", "Jon Snow returns to the Wall.", "episode-1.png", "53 min"],
  ["狮与玫瑰", "A royal celebration turns fragile.", "episode-2.png", "54 min"],
  ["破链者", "Daenerys negotiates beyond the sea.", "episode-3.png", "55 min", true],
  ["守誓者", "Old promises return with new cost.", "episode-4.png", "56 min"],
];

const root = document.querySelector("#viewRoot");
const navRoot = document.querySelector("#navList");
const viewTitle = document.querySelector("#viewTitle");
const viewEyebrow = document.querySelector("#viewEyebrow");
const viewCopy = document.querySelector("#viewCopy");

function renderNav(activeId) {
  navRoot.innerHTML = navItems.map((item) => `
    <button class="nav-button ${item.id === activeId ? "active" : ""}" type="button" data-view="${item.id}">
      ${item.label}
    </button>
  `).join("");
}

function section(title, action, content) {
  return `
    <section>
      <div class="section-head">
        <h2>${title}</h2>
        ${action ? `<a href="#">${action}</a>` : ""}
      </div>
      ${content}
    </section>
  `;
}

function renderContinueGrid() {
  return `
    <div class="continue-grid">
      ${continueItems.map((item) => `
        <article class="continue-card">
          <img src="${asset(item.image)}" alt="${item.title}">
          <div class="continue-body">
            <h3>${item.title}</h3>
            <p>${item.meta}</p>
            <div class="progress"><span style="width:${item.progress}%"></span></div>
          </div>
        </article>
      `).join("")}
    </div>
  `;
}

function renderMediaGrid(items = movies) {
  return `
    <div class="media-grid">
      ${items.map((item) => `
        <article class="media-card">
          <img src="${asset(item.image)}" alt="${item.title}">
          <div class="media-body">
            <h3>${item.title}</h3>
            <p>${item.meta} <span class="rating">★ ${item.rating}</span></p>
          </div>
        </article>
      `).join("")}
    </div>
  `;
}

function renderHome() {
  root.innerHTML = `
    <section class="hero" style="--hero-image: url('${asset("hero-scene-clean.png")}')">
      <div class="hero-content">
        <p class="eyebrow">继续观看</p>
        <h2>北境余烬 S4 E3</h2>
        <p>上次看到 36:12。字幕、音轨、播放设备都已为客厅电视保留。</p>
        <div class="hero-actions">
          <button class="button primary" type="button">继续播放</button>
          <button class="button" type="button">查看详情</button>
        </div>
      </div>
      <div class="media-tags">
        <span class="tag gold">4K UHD</span>
        <span class="tag">HDR10</span>
        <span class="tag">HEVC</span>
        <span class="tag">Atmos</span>
        <span class="tag">中文字幕</span>
      </div>
      <img class="poster-float" src="${asset("poster-main.png")}" alt="北境余烬海报">
    </section>

    ${section("继续观看", "全部续播", renderContinueGrid())}
    ${section("新入库电影", "查看电影库", renderMediaGrid())}
    ${section("热门剧集与分类", "", `
      <div class="category-grid">
        ${categories.map(([name, count]) => `
          <article class="category-card">
            <h3>${name}</h3>
            <p>${count}</p>
          </article>
        `).join("")}
      </div>
    `)}
  `;
}

function renderDetail() {
  root.innerHTML = `
    <section class="hero" style="--hero-image: url('${asset("hero-scene-clean.png")}')">
      <div class="hero-content">
        <p class="eyebrow">剧集详情</p>
        <h2>北境余烬</h2>
        <p>2018 - 2024 · 5 季 · 50 集 · 55 min。系统会自动保存观看进度、字幕选择与播放设备偏好。</p>
        <div class="hero-actions">
          <button class="button primary" type="button">继续 S4 E3</button>
          <button class="button" type="button">从头播放</button>
        </div>
      </div>
      <div class="media-tags">
        <span class="tag">剧情</span>
        <span class="tag">奇幻</span>
        <span class="tag">冒险</span>
        <span class="tag gold">4K UHD</span>
        <span class="tag">HDR10</span>
        <span class="tag">HEVC</span>
      </div>
      <img class="poster-float" src="${asset("poster-main.png")}" alt="北境余烬海报">
    </section>

    <div class="detail-layout">
      <section class="detail-panel">
        <h2>概览</h2>
        <p>在长夏与长冬交替的大陆上，北境家族为城墙之外的古老威胁重新集结。</p>
        <div class="episode-list">
          ${episodes.map((episode, index) => `
            <article class="episode-row ${episode[4] ? "active" : ""}">
              <img src="${asset(episode[2])}" alt="${episode[0]}">
              <div>
                <h3>${index + 1}. ${episode[0]}</h3>
                <p>${episode[1]}</p>
              </div>
              <time>${episode[3]}</time>
              <span class="episode-dot" aria-hidden="true"></span>
            </article>
          `).join("")}
        </div>
      </section>

      <aside class="detail-panel">
        <h2>媒体信息</h2>
        <dl class="info-list">
          ${[
            ["状态", "继续观看"],
            ["首播", "2018-04-01"],
            ["网络", "HBO"],
            ["分级", "TV-MA"],
            ["视频", "HEVC 10-bit"],
            ["分辨率", "3840 × 2160"],
            ["音频", "Dolby TrueHD 7.1"],
            ["字幕", "English / 简中"],
            ["文件", "412.8 GB"],
          ].map(([key, value]) => `<div><dt>${key}</dt><dd>${value}</dd></div>`).join("")}
        </dl>
      </aside>
    </div>
  `;
}

function renderAdmin() {
  const metrics = [
    ["存储使用", "18.7 TB", "56%", "--cyan"],
    ["在线用户", "8", "3 个活跃播放", "--green"],
    ["转码任务", "3", "GPU 42%", "--amber"],
    ["扫描队列", "128", "预计 12 分钟", "--primary"],
  ];

  root.innerHTML = `
    <section class="metric-grid">
      ${metrics.map(([label, value, hint, color]) => `
        <article class="metric-card" style="--cyan: var(${color})">
          <p>${label}</p>
          <h3>${value}</h3>
          <p>${hint}</p>
        </article>
      `).join("")}
    </section>

    <section class="admin-grid">
      <div class="detail-panel">
        <h2>存储池 ThinPool</h2>
        <div class="meter-list">
          ${[
            ["Movies", "8.6 TB", 100],
            ["TV Shows", "4.2 TB", 70],
            ["Music", "1.6 TB", 27],
            ["Photos", "1.1 TB", 22],
            ["Other", "3.2 TB", 53],
          ].map(([name, value, progress]) => `
            <div class="meter-row">
              <span>${name}</span>
              <strong>${value}</strong>
              <div class="progress"><span style="width:${progress}%"></span></div>
            </div>
          `).join("")}
        </div>
      </div>

      <div class="detail-panel">
        <h2>实时活动</h2>
        <ul class="activity-list">
          <li>Alex 继续播放 北境余烬 S4E3</li>
          <li>家庭影院请求 4K Direct Play</li>
          <li>服务器完成 42 个字幕匹配</li>
          <li>媒体扫描新增 18 部电影</li>
          <li>GPU 转码任务降为 3 个</li>
        </ul>
      </div>
    </section>

    ${section("用户与权限", "", `
      <div class="category-grid">
        ${[
          ["管理员", "全部权限"],
          ["家庭成员", "观看与收藏"],
          ["儿童模式", "限制分级内容"],
          ["访客", "临时访问"],
        ].map(([name, desc]) => `
          <article class="category-card">
            <h3>${name}</h3>
            <p>${desc}</p>
          </article>
        `).join("")}
      </div>
    `)}
  `;
}

function renderSystem() {
  root.innerHTML = `
    <section class="system-grid">
      <article class="system-card">
        <h2>AetherNAS 设计规范</h2>
        <div class="token-list">
          <div><strong>Surface</strong><p>base / panel / elevated，右侧信息与管理面板使用 elevated。</p></div>
          <div><strong>Shape</strong><p>卡片 8px，控制 12px，避免过度圆角。</p></div>
          <div><strong>Type</strong><p>标题 Noto Sans SC Bold，数字与编码标签使用 Inter。</p></div>
          <div><strong>States</strong><p>主操作用 primary，播放与转码状态用 cyan / green / amber。</p></div>
        </div>
      </article>

      <article class="system-card">
        <h2>适用组件</h2>
        <div class="component-list">
          ${[
            ["导航项", "侧边栏图标 + 标签 + 活跃态。"],
            ["搜索框", "媒体库搜索、筛选、快捷命令入口。"],
            ["媒体卡片", "海报/横封面、评分、进度与状态标签。"],
            ["信息面板", "详情页 metadata 与技术参数。"],
            ["管理指标", "存储、转码、用户、活动等后台指标。"],
            ["分类入口", "电影、剧集、动画、纪录片、收藏。"],
          ].map(([name, desc]) => `<div><strong>${name}</strong><p>${desc}</p></div>`).join("")}
        </div>
      </article>
    </section>
  `;
}

function setView(viewId) {
  const item = navItems.find((nav) => nav.id === viewId) ?? navItems[0];
  viewTitle.textContent = item.title;
  viewEyebrow.textContent = item.eyebrow;
  viewCopy.textContent = item.copy;
  renderNav(item.id);

  if (item.id === "detail") renderDetail();
  else if (item.id === "admin") renderAdmin();
  else if (item.id === "system") renderSystem();
  else renderHome();
}

navRoot.addEventListener("click", (event) => {
  const button = event.target.closest("[data-view]");
  if (!button) return;
  setView(button.dataset.view);
});

setView("home");
