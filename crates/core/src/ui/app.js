(function () {
  const COLORS = [
    '#0068FF', '#10B981', '#F59E0B', '#EF4444',
    '#8B5CF6', '#EC4899', '#06B6D4', '#6366F1'
  ];

  let state = {
    accounts: [],
    active_account_id: null,
    settings: {
      minimize_to_tray: true,
      start_with_windows: false,
      preload_all_accounts: true,
      enable_notifications: true,
      auto_hide_sidebar: false,
      sidebar_collapsed: false,
      theme: 'light',
      zoom_percent: 100
    },
    settings_open: false,
    is_installed: false,
    app_version: '1.0.0'
  };

  let selectedColor = COLORS[0];
  let toastTimer = null;
  let confirmCallback = null;

  const $ = (id) => document.getElementById(id);

  function send(payload) {
    try {
      window.ipc.postMessage(JSON.stringify(payload));
    } catch (err) {
      console.error('IPC error:', err);
    }
  }

  function initials(name) {
    const parts = (name || 'Z').trim().split(/\s+/).filter(Boolean);
    if (parts.length === 0) return 'Z';
    if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
    return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase();
  }

  function escapeHtml(str) {
    return String(str || '')
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  function showToast(kind, message) {
    const el = $('toast');
    el.className = 'toast ' + (kind || 'info');
    el.textContent = message;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => el.classList.add('hidden'), 3200);
  }

  function openConfirm(title, message, onOk) {
    $('confirmTitle').textContent = title;
    $('confirmMessage').textContent = message;
    confirmCallback = onOk;
    $('confirmModal').classList.remove('hidden');
  }

  function closeConfirm() {
    confirmCallback = null;
    $('confirmModal').classList.add('hidden');
  }

  function renderColorPicker() {
    const host = $('colorPicker');
    host.innerHTML = '';
    COLORS.forEach((c) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'color-dot' + (c === selectedColor ? ' selected' : '');
      btn.style.background = c;
      btn.addEventListener('click', () => {
        selectedColor = c;
        renderColorPicker();
      });
      host.appendChild(btn);
    });
  }

  function openAccountForm(account) {
    const card = $('accountFormCard');
    card.classList.remove('hidden');
    if (account) {
      $('formTitle').textContent = 'Chỉnh sửa tài khoản';
      $('editAccountId').value = account.id;
      $('inputName').value = account.name;
      $('inputNote').value = account.note || '';
      selectedColor = account.color || COLORS[0];
      $('btnSubmitForm').textContent = 'Lưu thay đổi';
    } else {
      $('formTitle').textContent = 'Thêm tài khoản Zalo mới';
      $('editAccountId').value = '';
      $('inputName').value = '';
      $('inputNote').value = '';
      selectedColor = COLORS[state.accounts.length % COLORS.length];
      $('btnSubmitForm').textContent = 'Thêm & Mở Zalo mới';
    }
    renderColorPicker();
    setTimeout(() => $('inputName').focus(), 40);
  }

  function closeAccountForm() {
    $('accountFormCard').classList.add('hidden');
  }

  function switchTab(tabName) {
    document.querySelectorAll('.tab-btn').forEach((b) => {
      b.classList.toggle('active', b.dataset.tab === tabName);
    });
    document.querySelectorAll('.tab-content').forEach((s) => {
      s.classList.toggle('active', s.id === 'tab-' + tabName);
    });
  }

  function render() {
    document.documentElement.setAttribute('data-theme', state.settings.theme || 'light');
    document.body.classList.toggle('settings-open', !!state.settings_open);
    document.body.classList.toggle('sidebar-collapsed', !!state.settings.sidebar_collapsed);
    $('btnSettings').classList.toggle('active', !!state.settings_open);

    // Show subtle red dot on 16px collapsed handle if ANY account has unread messages
    const totalUnread = state.accounts.reduce((sum, a) => sum + (a.unread_count || 0), 0);
    $('collapsedUnreadDot').classList.toggle('hidden', totalUnread === 0);
    $('collapsedStrip').title = totalUnread > 0
      ? `Mở thanh chuyển tài khoản (${totalUnread} tin mới) — Ctrl+B`
      : 'Mở thanh chuyển tài khoản Zalo (Ctrl+B)';

    // Render left rail
    const rail = $('accountRail');
    rail.innerHTML = '';
    state.accounts.forEach((acc, idx) => {
      const isActive = acc.id === state.active_account_id && !state.settings_open;
      const item = document.createElement('div');
      item.className = 'acc-item' + (isActive ? ' active' : '');

      const shortcutHint = idx < 9 ? ` (Ctrl+${idx + 1})` : '';
      const unreadBadge = acc.unread_count > 0
        ? `<span class="acc-unread">${acc.unread_count > 99 ? '99+' : acc.unread_count}</span>`
        : '';
      const onlineDot = acc.loaded ? '<span class="acc-online-dot"></span>' : '';

      item.innerHTML = `
        <span class="acc-indicator"></span>
        <button class="acc-avatar" style="background:${escapeHtml(acc.color)}" title="${escapeHtml(acc.name)}${shortcutHint}">
          ${escapeHtml(initials(acc.name))}
          ${onlineDot}
          ${unreadBadge}
        </button>
      `;
      item.querySelector('.acc-avatar').addEventListener('click', () => {
        send({ type: 'switch_account', id: acc.id });
      });
      rail.appendChild(item);
    });

    // Render account cards inside Settings
    $('accountCount').textContent = String(state.accounts.length);
    const grid = $('accountGrid');
    grid.innerHTML = '';
    state.accounts.forEach((acc, idx) => {
      const isCurrent = acc.id === state.active_account_id;
      const card = document.createElement('div');
      card.className = 'card account-card' + (isCurrent ? ' is-active' : '');

      const statusPill = isCurrent
        ? '<span class="pill active-pill">Đang chọn</span>'
        : acc.loaded
          ? '<span class="pill online-pill">Đang giữ phiên nền</span>'
          : '<span class="pill">Đã lưu đăng nhập</span>';

      const unreadPill = acc.unread_count > 0
        ? `<span class="pill unread-pill">${acc.unread_count} tin mới</span>`
        : '';

      const kbdPill = idx < 9 ? `<span class="pill">Ctrl+${idx + 1}</span>` : '';

      card.innerHTML = `
        <div class="card-top">
          <div class="card-avatar" style="background:${escapeHtml(acc.color)}">${escapeHtml(initials(acc.name))}</div>
          <div class="card-meta">
            <div class="card-title-row">
              <div class="card-name">${escapeHtml(acc.name)}</div>
            </div>
            <div class="card-note">${escapeHtml(acc.note || 'Hồ sơ cô lập riêng biệt')}</div>
          </div>
        </div>
        <div class="badge-row">
          ${statusPill}
          ${unreadPill}
          ${kbdPill}
        </div>
        <div class="card-actions">
          <button class="btn ${isCurrent ? 'btn-primary' : 'btn-secondary'} btn-sm js-switch">
            ${isCurrent ? 'Mở Zalo ngay' : 'Chuyển sang'}
          </button>
          <div class="card-mini-actions">
            <button class="btn btn-ghost js-edit" title="Đổi tên / màu">Sửa</button>
            <button class="btn btn-ghost js-reload" title="Tải lại trang">Tải lại</button>
            <button class="btn btn-ghost js-clear" title="Xóa cookie để quét mã QR mới">Quét lại QR</button>
            ${state.accounts.length > 1 ? '<button class="btn btn-ghost danger js-delete" title="Xóa tài khoản">Xóa</button>' : ''}
          </div>
        </div>
      `;

      card.querySelector('.js-switch').addEventListener('click', () => {
        send({ type: 'switch_account', id: acc.id });
      });
      card.querySelector('.js-edit').addEventListener('click', () => {
        openAccountForm(acc);
      });
      card.querySelector('.js-reload').addEventListener('click', () => {
        send({ type: 'reload_account', id: acc.id });
      });
      card.querySelector('.js-clear').addEventListener('click', () => {
        openConfirm(
          'Xóa phiên đăng nhập?',
          `Thao tác này sẽ đăng xuất và xóa dữ liệu phiên của "${acc.name}" để bạn quét mã QR mới. Các tài khoản Zalo khác vẫn được giữ nguyên.`,
          () => send({ type: 'clear_account_session', id: acc.id })
        );
      });
      const delBtn = card.querySelector('.js-delete');
      if (delBtn) {
        delBtn.addEventListener('click', () => {
          openConfirm(
            'Xóa tài khoản?',
            `Bạn có chắc muốn xóa "${acc.name}" khỏi danh sách? Dữ liệu phiên của tài khoản này sẽ bị xóa khỏi máy.`,
            () => send({ type: 'delete_account', id: acc.id })
          );
        });
      }

      grid.appendChild(card);
    });

    // Render system settings
    $('optEnableNotifications').checked = state.settings.enable_notifications !== false;
    $('optAutoHideSidebar').checked = !!state.settings.auto_hide_sidebar;
    $('optMinimizeTray').checked = !!state.settings.minimize_to_tray;
    $('optStartWindows').checked = !!state.settings.start_with_windows;
    $('optPreloadAll').checked = !!state.settings.preload_all_accounts;
    $('optZoom').value = String(state.settings.zoom_percent || 100);
    $('zoomLabel').textContent = (state.settings.zoom_percent || 100) + '%';
    $('appVersion').textContent = state.app_version || '1.0.0';

    document.querySelectorAll('#themeSegmented button').forEach((btn) => {
      btn.classList.toggle('active', btn.dataset.theme === (state.settings.theme || 'light'));
    });

    if (state.is_installed) {
      $('installStatusText').textContent = 'Mutil-Zalo đã được cài đặt trên Windows (có Shortcut Desktop & Start Menu). Bạn có thể cài đè để cập nhật hoặc gỡ cài đặt.';
      $('btnInstallWindows').textContent = 'Cập nhật cài đặt';
      $('btnUninstallWindows').classList.remove('hidden');
    } else {
      $('installStatusText').textContent = 'Đang chạy trực tiếp từ 1 file duy nhất. Bấm nút bên phải để cài vào máy, tạo Shortcut Desktop & Start Menu.';
      $('btnInstallWindows').textContent = 'Cài đặt vào máy';
      $('btnUninstallWindows').classList.add('hidden');
    }
  }

  function pushSettingsPatch(patch) {
    const next = Object.assign({}, state.settings, patch);
    send({ type: 'update_settings', settings: next });
  }

  // Hook called by Rust backend
  window.__mutilZaloUpdate = function (payload) {
    state = payload;
    render();
    if (payload.toast && payload.toast.message) {
      showToast(payload.toast.kind, payload.toast.message);
    }
  };

  // Wire UI controls
  $('collapsedStrip').addEventListener('click', () => {
    send({ type: 'toggle_sidebar' });
  });

  $('btnCollapseSidebar').addEventListener('click', () => {
    send({ type: 'toggle_sidebar' });
  });

  $('btnHideToTray').addEventListener('click', () => {
    send({ type: 'minimize_to_tray' });
  });

  $('btnSettings').addEventListener('click', () => {
    send({ type: 'set_settings_open', open: !state.settings_open });
  });

  $('btnBackToZalo').addEventListener('click', () => {
    send({ type: 'set_settings_open', open: false });
  });

  $('btnQuickAdd').addEventListener('click', () => {
    switchTab('accounts');
    if (!state.settings_open) {
      send({ type: 'set_settings_open', open: true });
    }
    openAccountForm(null);
  });

  $('btnReload').addEventListener('click', () => {
    if (state.active_account_id) {
      send({ type: 'reload_account', id: state.active_account_id });
    }
  });

  $('btnShowAddForm').addEventListener('click', () => openAccountForm(null));
  $('btnCloseForm').addEventListener('click', closeAccountForm);
  $('btnCancelForm').addEventListener('click', closeAccountForm);

  $('accountForm').addEventListener('submit', (e) => {
    e.preventDefault();
    const id = $('editAccountId').value.trim();
    const name = $('inputName').value.trim();
    const note = $('inputNote').value.trim();
    if (!name) return;

    if (id) {
      send({ type: 'update_account', id, name, note, color: selectedColor });
    } else {
      send({ type: 'add_account', name, note, color: selectedColor });
    }
    closeAccountForm();
  });

  document.querySelectorAll('.tab-btn').forEach((btn) => {
    btn.addEventListener('click', () => switchTab(btn.dataset.tab));
  });

  $('optEnableNotifications').addEventListener('change', (e) => {
    pushSettingsPatch({ enable_notifications: e.target.checked });
  });
  $('btnTestNotify').addEventListener('click', () => {
    send({ type: 'test_notification' });
  });
  $('optAutoHideSidebar').addEventListener('change', (e) => {
    pushSettingsPatch({ auto_hide_sidebar: e.target.checked });
  });
  $('optMinimizeTray').addEventListener('change', (e) => {
    pushSettingsPatch({ minimize_to_tray: e.target.checked });
  });
  $('optStartWindows').addEventListener('change', (e) => {
    pushSettingsPatch({ start_with_windows: e.target.checked });
  });
  $('optPreloadAll').addEventListener('change', (e) => {
    pushSettingsPatch({ preload_all_accounts: e.target.checked });
  });

  document.querySelectorAll('#themeSegmented button').forEach((btn) => {
    btn.addEventListener('click', () => pushSettingsPatch({ theme: btn.dataset.theme }));
  });

  $('optZoom').addEventListener('input', (e) => {
    $('zoomLabel').textContent = e.target.value + '%';
  });
  $('optZoom').addEventListener('change', (e) => {
    pushSettingsPatch({ zoom_percent: parseInt(e.target.value, 10) || 100 });
  });
  $('btnResetZoom').addEventListener('click', () => {
    pushSettingsPatch({ zoom_percent: 100 });
  });

  $('btnOpenDataDir').addEventListener('click', () => send({ type: 'open_data_folder' }));
  $('btnInstallWindows').addEventListener('click', () => send({ type: 'install_to_windows' }));
  $('btnUninstallWindows').addEventListener('click', () => {
    openConfirm(
      'Gỡ cài đặt Mutil-Zalo?',
      'Gỡ Shortcut Desktop, Start Menu và mục ứng dụng trong Windows. Dữ liệu phiên Zalo vẫn được giữ an toàn.',
      () => send({ type: 'uninstall_from_windows' })
    );
  });

  $('btnConfirmCancel').addEventListener('click', closeConfirm);
  $('btnConfirmOk').addEventListener('click', () => {
    const cb = confirmCallback;
    closeConfirm();
    if (cb) cb();
  });

  // Keyboard shortcuts
  window.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      if (!$('confirmModal').classList.contains('hidden')) {
        closeConfirm();
      } else if (state.settings_open) {
        send({ type: 'set_settings_open', open: false });
      }
      return;
    }
    if (e.ctrlKey && !e.altKey && !e.shiftKey) {
      if (e.key >= '1' && e.key <= '9') {
        const idx = parseInt(e.key, 10) - 1;
        if (state.accounts[idx]) {
          e.preventDefault();
          send({ type: 'switch_account', id: state.accounts[idx].id });
        }
      } else if (e.key === ',') {
        e.preventDefault();
        send({ type: 'set_settings_open', open: !state.settings_open });
      } else if (e.key === 'b' || e.key === 'B') {
        e.preventDefault();
        send({ type: 'toggle_sidebar' });
      }
    }
  });

  renderColorPicker();
  send({ type: 'ready' });
})();
