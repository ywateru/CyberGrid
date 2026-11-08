'use strict';
const copyStatus = document.querySelector('#copy-status');
function bindCopy(button, content) {
  button.addEventListener('click', async () => {
    const code = content();
    if (!code) return;
    const previous = button.textContent;
    try {
      if (!navigator.clipboard?.writeText) throw new Error('Clipboard unavailable');
      await navigator.clipboard.writeText(code.textContent);
      button.textContent = '已复制';
      if (copyStatus) copyStatus.textContent = '内容已复制到剪贴板。';
    } catch {
      const selection = window.getSelection();
      const range = document.createRange();
      range.selectNodeContents(code);
      selection?.removeAllRanges();
      selection?.addRange(range);
      button.textContent = '已选中内容';
      if (copyStatus) copyStatus.textContent = '未能自动复制，内容已选中，请使用系统复制操作。';
    }
    window.setTimeout(() => { button.textContent = previous; }, 2600);
  });
}
for (const button of document.querySelectorAll('[data-copy]')) bindCopy(button, () => document.getElementById(button.dataset.copy));
