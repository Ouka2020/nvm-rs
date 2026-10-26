activate_symlink_success = node {$version} is activated.
activate_symlink_is_activated=node is already activated.
activate_symlink_no_version=no node version installed

deactivate_symlink_is_deactivated=node is already deactivated.
deactivate_symlink_success=node is deactivated.

display_current_version=current version is {$version}
display_current_none=No current version.
display_current_tips=Tips:
display_current_tips_use={" "}* Run 'nvm use x.x.x' to set a version.
display_current_tips_ls_local={" "}* Run 'nvm ls' to list available versions locally.
display_current_tips_ls_remote={" "}* Run 'nvm ls --remote' to list available versions online.
display_current_tips_install_latest={" "}* Run 'nvm install latest' to install the latest version.

npm_mirror_current=Current NpmMirror: {$mirror}
npm_mirror_none=No NpmMirror set.
node_mirror_current=Current NodeMirror: {$mirror}
node_mirror_none=No NodeMirror set.
root_current=Current Root: {$root}
root_none=No Root set.

setup_finished=Finished. Please restart the console to update the environment.

checksum_prompt=checksum...
checksum_failed=failed.
checksum_ok=OK.
checksum_skip=skip.
install_success=install {$version} completed.

list_local_none=No versions are installed.
list_local_in_use={" "}<- In use
list_remote_note={" "}* Note: The list only shows the latest {$count} versions. Visit https://nodejs.org/en/ for more info.

switch_success=switch to {$version} success.
uninstall_success=uninstall {$version} completed.

root_path_not_exist=path {$path} does not exist
root_path_not_dir=path {$path} is not a directory

install_version_exists=version {$version} already installed
install_invalid_url=invalid url: {$url}
install_checksum_failed=sha256 checksum failed: {$file}
checksum_not_found=checksum not found: {$package}
extract_illegal_path=illegal path in archive: {$path}

switch_version_not_installed=version {$version} not installed
switch_version_already_used=version {$version} is already used

uninstall_need_exact_version=The version must be a specific version. Can not use 'latest' or 'lts' to uninstall.
uninstall_version_not_installed=version {$version} not installed
uninstall_version_in_use=version {$version} is in use, can not uninstall it

version_latest_not_found=No latest version found.
version_lts_not_found=No LTS version found.
version_not_exists=version {$version} not exists.
