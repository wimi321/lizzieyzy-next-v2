---
id: P3-45
title: 工程线：网页旁观页面的安全头——CSP、禁止被嵌入
depends_on: [P3-44]
paths:
  - crates/app-core/src/web_board/**
  - apps/desktop/src-tauri/src/web_board_assets.rs
  - apps/desktop/src-tauri/src/web_board_assets/**
  - docs/ARCHITECTURE.md
---

## 目标

网页旁观（P3-32、P3-33）把一个页面发给同一局域网里的浏览器。2026-11-19 复查 Tauri 权限和 CSP 时发现：

- 桌面窗口的 CSP 由 Tauri 在自己的协议里加上；网页旁观的服务器从 asset resolver 取出同一批文件，但不带这个 CSP。所以旁观页没有 CSP。
- 旁观页也没有禁止被别的网页嵌入（frame）。局域网里的恶意网页可以把带口令的地址嵌进去，诱导旁观者点击（试下会改动临时棋盘）。

现在的响应头只有 `x-content-type-options: nosniff`、`referrer-policy: no-referrer` 和 `cache-control: no-store`。

## 要做

1. 旁观页的 HTML 和静态文件的响应都加上：
   - `content-security-policy`，和桌面窗口的规则一致，再按旁观页的需要收紧：
     - `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' blob:`；
     - `connect-src` 只允许这个页面自己的 WebSocket 地址（`ws://` 加请求的 Host）。浏览器对 `'self'` 是否包括 `ws:` 的支持不一致，所以写明确的地址；
     - `frame-ancestors 'none'; base-uri 'none'; form-action 'none'; object-src 'none'`。
   - `x-frame-options: DENY`，给不认 `frame-ancestors` 的旧浏览器用。
   - Host 头不合法（不是 `主机:端口` 的形式，或者含有空白、引号、分号等不能放进 CSP 的字符）时，返回 400，不拼进 CSP。
2. WebSocket 升级的响应不需要这些头，保持不变。
3. 测试：
   - app-core 的旁观服务测试：HTML 和静态文件的响应带上面每个头，`connect-src` 用的是请求的 Host；非法 Host 返回 400；WebSocket 照常升级。
   - 页面在这个 CSP 下能正常显示、能连上 WebSocket：这一项由维护者在旁观页的网页 E2E（`e2e/web-page.spec.ts`，界面部分）里用同样的头验证，不在本卡里。汇报里写出最终的 CSP 字符串。
4. ARCHITECTURE 网页旁观一节补上这些响应头。

## 不做

- HTTPS、外网访问。
- 改口令的生成和校验方式。

## 验收标准

- 上面的测试在改之前失败、改之后通过。
- `scripts/verify.sh` 通过；`check_budgets.py` 通过。
