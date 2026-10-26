activate_symlink_success = node {$version} 已激活。
activate_symlink_is_activated=node 已处于激活状态。
activate_symlink_no_version=尚未安装任何 node 版本。

deactivate_symlink_is_deactivated=node 已处于停用状态。
deactivate_symlink_success=node 已停用。

display_current_version=当前版本为 {$version}。
display_current_none=当前没有已启用的版本。
display_current_tips=提示：
display_current_tips_use={" "}* 运行 'nvm use x.x.x' 设置要使用的版本。
display_current_tips_ls_local={" "}* 运行 'nvm ls' 查看本地已安装的版本。
display_current_tips_ls_remote={" "}* 运行 'nvm ls --remote' 查看远程可用版本。
display_current_tips_install_latest={" "}* 运行 'nvm install latest' 安装最新版本。

npm_mirror_current=当前 NPM 镜像源：{$mirror}
npm_mirror_none=未设置 NPM 镜像源。
node_mirror_current=当前 Node 镜像源：{$mirror}
node_mirror_none=未设置 Node 镜像源。
root_current=当前根目录：{$root}
root_none=未设置根目录。

setup_finished=设置完成。请重启控制台以更新环境变量。

checksum_prompt=校验中...
checksum_failed=失败。
checksum_ok=成功。
checksum_skip=已跳过。
install_success={$version} 安装完成。

list_local_none=尚未安装任何版本。
list_local_in_use={" "}<- 当前使用中
list_remote_note={" "}* 注意：列表仅显示最新的 {$count} 个版本，更多信息请访问 https://nodejs.org/en/。

switch_success=已切换到 {$version}。
uninstall_success={$version} 卸载完成。

root_path_not_exist=路径 {$path} 不存在。
root_path_not_dir=路径 {$path} 不是目录。

install_version_exists=版本 {$version} 已安装。
install_invalid_url=无效的 URL：{$url}
install_checksum_failed=SHA256 校验失败：{$file}
checksum_not_found=未找到校验和：{$package}
extract_illegal_path=压缩包内存在非法路径：{$path}

switch_version_not_installed=版本 {$version} 未安装。
switch_version_already_used=版本 {$version} 已在使用中。

uninstall_need_exact_version=卸载时版本必须是具体版本号，不能使用 'latest' 或 'lts'。
uninstall_version_not_installed=版本 {$version} 未安装。
uninstall_version_in_use=版本 {$version} 正在使用中，无法卸载。

version_latest_not_found=未找到最新版本。
version_lts_not_found=未找到 LTS 版本。
version_not_exists=版本 {$version} 不存在。
