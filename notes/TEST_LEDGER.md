# Test ledger

    Every Python test function, by module, and what became of it. Generated from the
    sources; regenerate after changing ignore reasons.

    - `ported`: same test, same assertions.
    - `needs-design`: fully ported but blocked on a small API decision; the reason is
      the `#[ignore]` text and a `TODO(port)` comment marks the spot.
    - `N/A`: no Rust meaning.

    | Module | Python tests | ported | needs-design | N/A |
    |---|---|---|---|---|
    | test_answersfile | 9 | 8 | 1 | 0 |
| test_answersfile_templating | 3 | 3 | 0 | 0 |
| test_check_update | 13 | 13 | 0 | 0 |
| test_cleanup | 8 | 5 | 3 | 0 |
| test_cli | 14 | 10 | 4 | 0 |
| test_complex_questions | 10 | 10 | 0 | 0 |
| test_conditional_file_name | 5 | 5 | 0 | 0 |
| test_config | 25 | 22 | 1 | 2 |
| test_context | 4 | 3 | 1 | 0 |
| test_copy | 42 | 38 | 4 | 0 |
| test_demo_update_tasks | 1 | 1 | 0 | 0 |
| test_dirty_local | 8 | 6 | 2 | 0 |
| test_dynamic_file_structures | 7 | 7 | 0 | 0 |
| test_empty_suffix | 2 | 2 | 0 | 0 |
| test_exclude | 15 | 15 | 0 | 0 |
| test_imports | 4 | 4 | 0 | 0 |
| test_interrupts | 2 | 2 | 0 | 0 |
| test_jinja2_env | 3 | 0 | 0 | 3 |
| test_jinja2_extensions | 4 | 3 | 1 | 0 |
| test_jinja2_undefined | 4 | 4 | 0 | 0 |
| test_legacy_migration | 4 | 4 | 0 | 0 |
| test_migrations | 14 | 14 | 0 | 0 |
| test_minimum_version | 6 | 0 | 6 | 0 |
| test_normal_jinja2 | 2 | 2 | 0 | 0 |
| test_output | 10 | 10 | 0 | 0 |
| test_prompt | 33 | 30 | 3 | 0 |
| test_recopy | 6 | 6 | 0 | 0 |
| test_settings | 11 | 5 | 6 | 0 |
| test_settings_deprecated | 9 | 3 | 6 | 0 |
| test_subdirectory | 9 | 9 | 0 | 0 |
| test_symlinks | 19 | 18 | 1 | 0 |
| test_tasks | 8 | 7 | 1 | 0 |
| test_templated_prompt | 10 | 10 | 0 | 0 |
| test_tmpdir | 2 | 0 | 2 | 0 |
| test_tools | 6 | 5 | 0 | 1 |
| test_unsafe | 4 | 2 | 2 | 0 |
| test_updatediff | 36 | 35 | 1 | 0 |
| test_vcs | 22 | 20 | 2 | 0 |
| **total** | **394** | **341** | **47** | **6** |

## Tests that are not plain ports

| Module | Test | Status | Reason |
|---|---|---|---|
| test_answersfile | test_external_data_with_unicode_characters | needs-design | cannot override the default text encoding (monkeypatch of io.text_encoding) |
| test_cleanup | test_cleanup_on_interrupt_during_copy_task | needs-design | SIGINT from a task and tempfile.tempdir/TMPDIR isolation |
| test_cleanup | test_cleanup_on_interrupt_during_update_task | needs-design | SIGINT from a task and tempfile.tempdir/TMPDIR isolation |
| test_cleanup | test_cleanup_on_interrupt_during_git_clone | needs-design | patches plumbing's Popen to raise SIGINT during git clone |
| test_cli | test_read_utf_data_file | needs-design | monkeypatches io.text_encoding, which has no Rust equivalent |
| test_cli | test_copy_help | needs-design | help output is rendered at a fixed width of 80 |
| test_cli | test_update_help | needs-design | help output is rendered at a fixed width of 80 |
| test_cli | test_check_update_help | needs-design | help output is rendered at a fixed width of 80 |
| test_config | test_authorization_does_not_enable_python_yaml_tags | needs-design | finalize side effect is cwd-relative; cannot change cwd |
| test_config | test_flags_bad_data | N/A | pydantic type errors; rejected at compile time in Rust |
| test_config | test_flags_extra_fails | N/A | unknown keyword argument; rejected at compile time in Rust |
| test_context | test_no_path_variables | needs-design | Python Jinja extensions cannot be loaded |
| test_copy | test_copy_without_git | needs-design | cannot make `git` look missing (monkeypatches `LocalMachine.which`) |
| test_copy | test_multiselect_choices_preserve_order | needs-design | no YAML parser available to read back `q.yml` |
| test_copy | test_multiselect_choices_with_templated_default_value | needs-design | no YAML parser available to read back `q.yml` |
| test_copy | test_warn_if_chmod_raises_permission_error | needs-design | cannot make `chmod` fail (patches `Path.chmod`) |
| test_dirty_local | test_copy_dirty_head_with_gpg | needs-design | per-test git config |
| test_dirty_local | test_update_with_gpg_sign | needs-design | per-test git config |
| test_jinja2_env | test_purepath_parser_attribute_is_unsafe | N/A | probes Python objects (PurePath.parser, SettingsModel) inside the Jinja sandbox |
| test_jinja2_env | test_purepath_parser_attribute_yields_undefined | N/A | probes Python objects (PurePath.parser, SettingsModel) inside the Jinja sandbox |
| test_jinja2_env | test_settings_capability_is_unsafe | N/A | probes Python objects (PurePath.parser, SettingsModel) inside the Jinja sandbox |
| test_jinja2_extensions | test_additional_jinja2_extensions | needs-design | Python Jinja extensions cannot be loaded |
| test_minimum_version | test_version_less_than_required | needs-design | copier.__version__ monkeypatch; the version is a const (COPIER_VERSION) |
| test_minimum_version | test_version_equal_required | needs-design | copier.__version__ monkeypatch; the version is a const (COPIER_VERSION) |
| test_minimum_version | test_version_greater_than_required | needs-design | copier.__version__ monkeypatch; the version is a const (COPIER_VERSION) |
| test_minimum_version | test_minimum_version_update | needs-design | copier.__version__ monkeypatch; the version is a const (COPIER_VERSION) |
| test_minimum_version | test_version_0_0_0_ignored | needs-design | patches copier._template.copier_version and turns warnings into errors |
| test_minimum_version | test_version_bigger_major_warning | needs-design | copier.__version__ monkeypatch and warnings turned into errors |
| test_prompt | test_path_completion | needs-design | requires a pty |
| test_prompt | test_when | needs-design | YAML parsing in tests |
| test_prompt | test_prompt_does_not_capture_mouse | needs-design | requires a pty |
| test_settings | test_load_settings_from_default_location | needs-design | settings location override |
| test_settings | test_default_windows_settings_path | needs-design | settings location override |
| test_settings | test_load_settings_from_default_location_dont_exist | needs-design | settings location override |
| test_settings | test_settings_from_env_location | needs-design | settings location override |
| test_settings | test_load_settings_from_param | needs-design | settings location override |
| test_settings | test_load_settings_env_defined_but_missing | needs-design | settings location override |
| test_settings_deprecated | test_settings_from_default_location | needs-design | settings location override |
| test_settings_deprecated | test_default_windows_settings_path | needs-design | settings location override |
| test_settings_deprecated | test_settings_from_default_location_dont_exists | needs-design | settings location override |
| test_settings_deprecated | test_settings_from_env_location | needs-design | settings location override |
| test_settings_deprecated | test_settings_from_param | needs-design | settings location override |
| test_settings_deprecated | test_settings_defined_but_missing | needs-design | settings location override |
| test_symlinks | test_resolve_relative_symlink_outside_template_root_raises_error | needs-design | depends on copier's internal template clone location |
| test_tasks | test_os_specific_tasks | needs-design | operating system override |
| test_tmpdir | test_api | needs-design | tempfile.tempdir monkeypatch; temp clone dirs cannot be redirected |
| test_tmpdir | test_cli | needs-design | tempfile.tempdir monkeypatch; temp clone dirs cannot be redirected |
| test_tools | test_types | N/A | runs mypy on the Python sources |
| test_unsafe | test_copy_cli | needs-design | the CLI cannot be given Settings trust |
| test_unsafe | test_update_cli | needs-design | the CLI cannot be given Settings trust |
| test_updatediff | test_commit_hooks_respected | needs-design | requires the pre-commit tool and network |
| test_vcs | test_remote_clone_submodule_with_moved_url | needs-design | gitconfig fixture needed to allow file protocol for submodules |
| test_vcs | test_local_dirty_clone | needs-design | gitconfig fixture needed to set core.fsmonitor |
