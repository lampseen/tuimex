# What is proven

What these pages and the README say tuimex does is listed here, each line
beside the test or the CI job that holds it. What no test reaches is
listed at the end, as plainly.

A name such as `no_key_quits_and_the_two_commands_do` is a test in the
repository: search for it to read what it does. `make check` runs all of
them except the ones marked *by `make real-server`*, which need two mail
servers in containers and run in the CI job `real-server`. A test checks
that every name on this page is a test that exists.

## Starting, and the terminal

| what is true | what proves it |
|---|---|
| `tuimex --fixtures` shows the list, drawn from a synthetic mailbox, and `:q` leaves the terminal as it was | `fixtures_opens_the_list_and_the_quit_command_leaves_the_terminal_as_it_was` |
| Resizing the terminal redraws the list at the new size | `a_resize_redraws_the_list_at_the_new_size` |
| Each frame is drawn inside one synchronised update | `every_frame_is_one_synchronised_update` |
| Colours and glyphs follow what the terminal advertises, unless `--colors` or `--glyphs` says otherwise | `colours_and_glyphs_follow_the_environment_unless_a_flag_says_otherwise` |
| A crash puts the terminal back before its message prints | `a_panic_restores_the_terminal_before_its_message_prints` |
| With no config and no terminal to ask on, or with a config that names no account, it stops and says why, instead of showing a demo | `without_fixtures_it_says_why_it_stops_and_draws_nothing`, `a_config_with_no_account_stops_it_and_a_section_that_is_not_there_does_too` |
| Before there is a list the screen says `Opening mail`, and an index that has to be rebuilt after an upgrade is counted there, message by message | `a_rebuild_on_open_says_how_many_messages_it_has_indexed_of_how_many`, `an_index_to_rebuild_is_counted_on_the_screen_before_the_list_is_drawn`, and the `loading` goldens |
| `^l` clears and redraws, also on a terminal that does not say where its cursor is | `ctrl_l_redraws_without_asking_the_terminal_anything` |
| No key quits: Esc goes back, and `:q` and `:qa` leave | `no_key_quits_and_the_two_commands_do` |

## Setup, signing in, and doctor

| what is true | what proves it |
|---|---|
| On a terminal with no config, `tuimex` runs setup: an account is added only once its IMAP and SMTP servers have taken the sign-in, a certificate the system does not trust is shown and, accepted, pinned for both; then the interface opens. `tuimex setup` adds further accounts after what the config held, which is kept to the byte | `setup_adds_two_accounts_from_nothing_and_sync_and_doctor_then_need_no_answers`, by `make real-server`; `a_first_launch_on_a_terminal_runs_setup_and_opens_nothing_when_setup_is_left`; `a_second_account_is_added_after_what_was_there_which_is_kept_to_the_byte` |
| A password typed into setup is not echoed, and is kept in the keyring or not at all | `setup_adds_two_accounts_from_nothing_and_sync_and_doctor_then_need_no_answers` (echo, and the refusal with no keyring service); `an_account_is_added_from_nothing_once_both_servers_have_taken_the_password`, on a keyring in memory. The system's keyring itself: no test |
| Gmail signs in through a browser with a client of your own: a loopback redirect, PKCE, the state checked, and on a machine with no browser the pasted address; two accounts share one client; an access token is replaced when it ends, and a revoked sign-in says to run setup | `two_gmail_accounts_sign_in_through_a_browser_and_share_one_client`, `with_no_browser_the_address_is_printed_and_the_pasted_redirect_or_code_is_taken`, `a_code_is_not_exchanged_by_a_sign_in_that_did_not_ask_for_it`, `a_token_that_ends_mid_session_is_replaced_unasked_and_a_revoked_sign_in_stops_naming_setup`, all against a stand-in for Google; `a_bearer_token_signs_in_over_imap_and_smtp_and_a_wrong_one_is_a_refused_sign_in`, by `make real-server`, against a server that takes the account's password as the token. Google itself: no test |
| `tuimex doctor` checks every account end to end and this machine, runs nothing it is only to find, and exits non-zero when something is wrong | `a_working_setup_is_reported_line_by_line_and_nothing_is_wrong`, `each_thing_that_is_broken_is_named_with_what_mends_it_and_the_run_fails`, `the_agent_command_is_found_and_never_run_and_a_custom_one_is_not_vouched_for`; by `make real-server`, in `setup_adds_two_accounts_from_nothing_and_sync_and_doctor_then_need_no_answers` |
| `tuimex doctor --redact` prints the same report line for line with every account name, address, user name, server host, certificate fingerprint and the home directory replaced by a placeholder that is the same wherever the value recurs, inside a server's own words too, and no name inside a longer word | `redact_replaces_each_kind_of_value_where_it_stands_whole_and_nothing_else`; over a working setup and each broken one in `a_working_setup_is_reported_line_by_line_and_nothing_is_wrong` and `each_thing_that_is_broken_is_named_with_what_mends_it_and_the_run_fails`; through the binary in `doctor_redacted_prints_the_same_report_with_no_name_address_host_or_home_in_it`. What the config does not name, such as a folder in a path: not replaced |
| No password, client secret or token that setup, the sign-in or doctor handled is in a file or in anything printed, with `--redact` or without | `a_secret_never_reaches_a_file_or_anything_setup_or_doctor_says`, a named CI job |

## Syncing with a server

| what is true | what proves it |
|---|---|
| Over three accounts of a real IMAP server, two of them named alike, the list shows each row's account; a message added on the server appears with no key pressed; `e` archives and `u` brings it back, on the server; `/` and Enter find a word of a body in every account and in a folder that is no inbox; `gt` shows each account alone; an action the server refuses is put back and said; `:q` leaves with 0 | `the_interface_shows_three_accounts_follows_the_server_changes_it_and_searches_them_all`, by `make real-server` |
| `tuimex sync` fills a Maildir from an IMAP server, and run again transfers nothing | `tuimex_sync_fills_a_maildir_and_a_second_run_transfers_nothing`, run by `make real-server` against servers in containers, not by `make check` |
| A first sync shows how far it has got, and an account that cannot sync says so and marks its rows | `a_first_sync_says_how_far_it_has_got_after_each_batch_and_a_later_pass_says_nothing`, `an_account_out_of_step_with_its_server_says_so_until_it_syncs` |
| A refusal names the action it refuses, however late, and only that action is put back; what answers no action of the run is said as the engine worded it | `a_refused_move_is_undone_locally_and_reported`, `a_change_an_earlier_run_journaled_is_refused_as_a_notice_and_named_by_no_ask`, `a_refusal_puts_back_only_what_the_refused_action_changed` |
| `:empty-trash` deletes by name and never sends a bare `EXPUNGE` | `emptying_the_trash_deletes_by_name_and_only_what_was_in_it`, against a server in memory |
| A server's certificate is always checked; one you pinned is the only one accepted for that account | `a_certificate_that_is_not_the_pinned_one_or_not_pinned_at_all_is_refused_before_signing_in`, by `make real-server` |
| A server that will not start TLS is sent no password | `a_server_that_does_not_offer_starttls_is_left_before_anything_else_is_sent` |
| A password reaches no file tuimex writes and nothing it says | `a_secret_never_reaches_a_file_or_anything_the_engine_says` |

## Files and the config

| what is true | what proves it |
|---|---|
| Every file and directory a run creates is readable by you alone | `everything_a_run_creates_is_owner_only` |
| One module creates files; no other code can | `only_the_files_module_creates_a_file` |
| The log goes to `~/.local/state/tuimex/tuimex.log` and never to the terminal | `the_log_goes_to_the_state_directory_and_never_to_the_terminal` |
| The config is `~/.config/tuimex/config.toml` or the file `--config` names, never one in the current directory | `the_config_is_never_read_from_the_current_directory` |
| A key the config does not have is an error naming the key and its line, and never repeats a value | `an_unknown_key_is_an_error_naming_the_key_and_its_line`, `an_error_never_quotes_a_value_from_the_file` |
| No colour is in the repository but the built-in palette's | `only_the_built_in_palette_is_shipped` |
| `tuimex palette import <theme>` writes a palette from a Ghostty or Alacritty theme, and the list is drawn in it once `ui.palette` names it | `an_imported_palette_is_owner_only_and_colours_the_list_once_the_config_names_it`, `a_ghostty_theme_maps_by_the_table`, `an_alacritty_theme_maps_by_the_table` |

## The list

| what is true | what proves it |
|---|---|
| The list matches the design cell for cell, in truecolor with Unicode and in sixteen colours with ASCII | the `list*` tests of [`crates/tuimex-ui/tests/golden.rs`](https://github.com/lampseen/tuimex/blob/main/crates/tuimex-ui/tests/golden.rs), and `no_output_goes_undrawn` |
| The keys are the design's, and [`docs/keymap.md`](https://github.com/lampseen/tuimex/blob/main/docs/keymap.md) is written from the same table | `the_table_is_the_design_s_keymap`, `docs_keymap_md_is_generated_from_the_table` |
| Every key of the list, compose, the command line and a confirm does something | `every_key_of_the_table_does_something`, and on a pseudo-terminal `a_session_of_keys_moves_marks_archives_undoes_finds_searches_trashes_and_shows_help` |
| The command line, the help, the move prompt and the archive, trash, move and send flows match the design cell for cell | `help`, `move_prompt`, `command_line`, `archive_undo`, `trash_batch`, `move_refused`, `send_sent` and `send_undone` of the golden tests |
| Archive, trash, move, flag and read take the marked rows or the selected one, a folded thread whole, and `u` reverses the last | `marked_rows_are_taken_wherever_the_cursor_is_and_a_marked_thread_is_taken_whole`, `a_thread_unfolds_and_folds_and_an_action_takes_what_the_status_line_says`, `flag_and_read_toggle_and_u_reverses_the_last_action_first` |
| A change that is refused is put back and reported | `a_refusal_puts_back_only_what_the_refused_action_changed`, `move_refused` |
| `/` finds among the rows here in the query grammar, every term of it | `slash_finds_among_the_rows_here_as_it_is_typed_in_the_query_grammar`, `every_term_of_the_grammar_is_asked_of_the_message`, `has_unsubscribe_matches_what_says_how_to_leave_its_list` |
| `/` and Enter search all mail: the store answers from every account and folder, bodies included, and Esc returns to the row `/` was pressed on | `enter_on_a_filter_searches_all_mail_and_esc_returns_with_the_cursor_kept`, `a_search_finds_a_word_of_a_body_in_every_account_and_folder`, and the `search` goldens |
| `--section` and `:section` open a section by its name or short name, and one that is not there stops tuimex and says so | `a_section_is_opened_by_name_or_short_name_and_one_that_is_not_there_is_not`, and on a pseudo-terminal `a_config_with_no_account_stops_it_and_a_section_that_is_not_there_does_too` |
| The preview sits under the list, or beside it from 160 columns; `P`, `:preview` and `ui.preview` move it, Alt-j or Alt-l focuses it and it then scrolls | `the_preview_is_beside_the_list_in_a_wide_terminal_and_capital_p_and_the_command_move_it`, `alt_j_focuses_the_preview_which_then_scrolls_while_actions_still_take_the_row`, `ui_preview_is_one_of_four_words_and_auto_when_left_out`, and on a pseudo-terminal `an_escape_and_a_letter_together_are_alt_and_apart_they_are_esc_then_the_letter` |
| The preview is the whole message: resting on a row reads it, a key held down reads none of the rows it passes, a message with no text here says it is being fetched, and the focused preview scrolls to its end | `the_preview_reads_the_message_the_cursor_rests_on_and_shows_all_of_it`, `the_preview_s_message_is_read_once_the_cursor_rests_and_not_while_a_key_is_held`, and the `list-preview-scrolled`, `list-preview-arriving` and `list-browser-ask` goldens |
| `B` on a row is the reader's `B` for that row's message, its body fetched first when it is not here; Enter on a draft opens compose on the draft as it was saved, with its files, its Bcc and its whole body | `capital_b_on_a_row_fetches_the_message_and_asks_the_reader_s_question`, `enter_on_a_draft_opens_it_in_compose_as_it_was_saved_and_not_in_the_reader`, `enter_on_a_stored_draft_opens_compose_with_its_fields_files_and_whole_body`, and the `list-drafts` golden |
| No word of a message or of its header is cut where it is read: paragraphs, headings, list items, quotes, signature lines and alt text wrap, a table too wide becomes its rows' cells parted by dots, recipients and files are packed whole and stop at two rows in the preview and six in the reader with how many more, and a notice wraps under its own text and keeps the address it warns about | `at_a_narrow_measure_every_kind_of_line_wraps_and_none_is_cut`, `a_table_too_wide_for_the_measure_is_each_row_s_cells_in_a_line_parted_by_dots`, `a_picture_s_placeholder_is_no_wider_than_the_measure_and_keeps_what_fits_of_its_label`, `a_notice_wraps_under_its_own_text_and_keeps_the_address_it_warns_about`, `recipients_are_packed_whole_and_stop_at_two_rows_in_the_preview_and_six_in_the_reader`, `an_address_wider_than_the_row_is_broken_and_every_other_is_kept_whole`, and the `list-wide-recipients` and `list-wide-preview-scrolled` goldens |
| With several accounts each row names its own in that account's colour, names that begin alike stay apart when cut short, and `gt` and `gT` show each account alone in turn | `three_accounts_have_three_names_in_three_colours_at_every_width_of_the_column`, `account_names_alike_at_their_start_stay_apart_when_cut`, `gt_steps_through_all_three_accounts_and_back_to_all_and_capital_t_steps_the_other_way`, and the `list-three-accounts` and `account-cycle` goldens |
| With a palette file loaded, the accounts take its accents in the order that keeps them furthest apart for normal, protan and deutan sight, and a row's words take a spare accent or, with none that would show, are told apart by brightness; `color = "…"` under an `[[account]]` fixes one account's | `separation_and_contrast_are_the_design_s_for_a_table_of_colour_pairs`, `a_loaded_palette_orders_the_account_colours_by_the_design_s_rule`, `the_words_take_the_free_accent_that_shows_or_fall_back_to_brightness`, `three_accounts_with_a_loaded_palette_take_the_order_its_colours_give`, `words_with_no_colour_that_would_show_are_told_apart_by_brightness`, `a_color_in_the_config_fixes_one_account_and_the_rest_keep_their_turn`, `an_account_s_color_is_one_of_the_four_accents_accounts_take` |

## The reader, links and files

| what is true | what proves it |
|---|---|
| Enter opens a message in the reader and marks it read; it scrolls, `n b` walk its thread, `J K` the list, and Esc returns with the cursor kept | `enter_opens_the_row_s_message_marks_it_read_and_esc_returns_with_the_cursor_kept`, `the_reader_scrolls_and_says_which_lines_show`, `n_and_b_walk_the_thread_and_capital_j_and_k_the_list` |
| The reader, its notices, the links and files over it and the invitation's card match the design cell for cell | `reader_plain`, `reader_plain_80_ascii`, `reader_html`, `reader_attachments`, `reader_invite`, `reader_notices`, `links`, `links_confirm`, `links_agent` and `agent_link` of the golden tests |
| Every key of the reader, its links, its files and an invitation does something | `every_key_of_the_table_does_something`, and on a pseudo-terminal `a_session_in_the_reader_scrolls_lists_files_and_confirms_a_deceptive_link` |
| A link shows the host it really opens; one that is neither web nor mail is shown and never opened | `a_link_of_another_scheme_is_shown_and_refused`, `a_link_s_row_shows_its_real_host_and_what_is_wrong_with_it` |
| A link whose text names another host, that carries a sign-in name, or whose host is a bare address or mixes alphabets opens only after `y` | `a_link_that_deceives_is_confirmed_first_and_names_its_real_host` |
| An honest link opens at once, without its tracking parameters | `an_honest_link_opens_at_once_without_its_tracking_parameters` |
| `:unsubscribe` opens an https page only after `y`; a `mailto` one opens compose for review and sends nothing | `unsubscribe_opens_an_https_page_only_after_a_yes`, `unsubscribe_is_never_automatic`, `a_mailto_link_and_a_mailto_unsubscribe_open_compose_and_send_nothing` |
| A file is saved under a safe name inside the download directory, owner-only, and never over another | `a_file_is_saved_under_a_safe_name_inside_the_download_directory`, `a_path_given_to_save_is_read_against_the_download_directory_and_never_written_over` |
| A file is handed to the system's program for its type only after a question naming the type, from a private copy removed at exit | `a_file_is_opened_from_a_private_copy_after_asking_and_removed_at_exit`; the system's opener itself is not run by any test |
| `B` and `:browser` write the sanitised, sealed page owner-only under the cache directory; a message with pictures on a server is asked about each time, naming the hosts, and a yes lets the copy load pictures from those hosts over https and nothing else; the copy goes at exit, or after a day | `the_browser_copy_is_the_sealed_page_written_owner_only_and_removed_at_exit`, `a_yes_to_the_pictures_allows_exactly_the_hosts_named_for_images_over_https`, `the_question_names_and_counts_only_the_pictures_a_yes_loads_among_a_script_and_a_stylesheet_host`, `a_yes_to_the_pictures_loads_no_tracker_and_a_message_with_only_trackers_is_not_asked`, `a_no_to_the_pictures_opens_the_copy_sealed`, `a_message_with_no_remote_pictures_opens_without_a_question`, `what_was_shown_more_than_a_day_ago_is_removed_at_start` |
| The browser copy keeps a message's layout (table sizes and alignment, classes, and the stylesheet rules that only lay out and colour) and nothing that fetches, runs or covers the page; a picture left on another host is a box of its declared size, counted in a line above the message | `a_stylesheet_keeps_the_rules_that_lay_a_message_out_and_loses_what_fetches_runs_or_covers`, `what_lays_a_table_out_is_kept_and_what_names_something_to_load_is_not`, `pictures_left_on_other_hosts_are_counted_above_the_message_and_drawn_as_quiet_boxes_of_their_size`; corpus: `layout-order-confirmation.eml`, `stylesheet-hostile.eml`, `remote-images-placeholder.eml`. How a browser draws it: no test |
| A picture a message attaches inline is given cells by its shape, at most 48 by 12, and a placeholder in them | `a_picture_takes_cells_by_its_shape_and_is_placed_once`, `reader_attachments` |
| Where the terminal has an image protocol (kitty, sixel, iTerm2) the picture's own bytes are handed to it over the placeholder; nothing is fetched. No test has a terminal to see the result on, and `--fixtures` has no picture bytes, so it has not been seen drawn | `an_attached_picture_is_sized_placed_and_drawn_from_the_message_s_own_bytes`, `a_terminal_with_a_protocol_gets_the_picture_over_its_placeholder`, `the_placeholder_stays_where_no_picture_can_be_drawn` |
| Only png, jpeg, gif and webp are decoded, never one over 8192 pixels on a side, and damaged bytes fail without a panic | `another_format_is_not_a_picture`, `a_picture_claiming_more_than_the_cap_is_never_decoded`, `damaged_bytes_decode_or_fail_and_never_panic` |
| A signed or encrypted message nobody verified says so and is never drawn as good; verifying is not built | `a_signed_message_without_a_verdict_claims_none` |
| The answers to an invitation are not built, say so and change nothing | `what_is_not_built_yet_says_so` |

## Writing and sending

| what is true | what proves it |
|---|---|
| `c`, `r`, `R` and `f` open compose from the list and the reader, new or quoting the message as mutt does, with the headers that keep a reply in its thread; on a saved draft they go on writing it | `compose_opens_from_the_keys_and_the_commands_alike`, `a_reply_from_the_reader_quotes_what_the_reader_shows`, `a_reply_quotes_the_message_under_who_wrote_it_and_when`, `a_reply_carries_the_headers_that_keep_it_in_its_thread`, `a_forward_is_the_message_between_two_markers_and_answers_nothing` |
| The identity is chosen on From, and the draft is filed and sent under that identity's account | `left_and_right_choose_the_identity_and_the_account_follows_it` |
| `Enter` on the body opens `$EDITOR` on an owner-only file under the cache, and the interface comes back around it | `the_body_is_written_in_editor_and_the_interface_comes_back_around_it`, `the_body_goes_through_the_editor_in_an_owner_only_file_that_is_then_removed` |
| `^a` attaches a file, completing its path | `ctrl_a_attaches_a_file_with_the_path_completed`, `a_path_completes_to_the_files_it_could_name` |
| An address that does not parse stops the summary and is named | `an_address_that_does_not_parse_stops_the_summary_and_is_named` |
| The send summary lists every recipient on a line and marks one whose domain is nowhere in the thread; only `y` sends | `a_recipient_from_outside_the_thread_is_marked_by_registrable_domain`, `only_y_sends_and_the_draft_is_handed_on_before_the_send` |
| `^d` saves the draft, and `Esc` asks whether to keep one that changed | `ctrl_d_saves_the_draft_into_drafts_and_hands_it_on`, `esc_asks_keep_or_discard_only_when_something_changed`, and on a pseudo-terminal `a_session_writes_a_message_reads_its_summary_and_saves_the_draft` |
| Compose, the send summary and the `:agent` screens match the design cell for cell | `compose`, `send_summary`, `compose_agent`, `agent_reply` and `agent_failed` of the golden tests |

## `:agent`, inside tuimex

| what is true | what proves it |
|---|---|
| `A` opens the agent's menu where there is something for it to do, sends nothing itself, and its pick runs the configured command: `r` on a row writes the reply in compose | `capital_a_opens_the_agent_s_menu_and_only_a_pick_sends`, `capital_a_asks_the_agent_about_a_link_from_the_list_and_from_its_question`, `without_an_agent_capital_a_says_what_is_missing_and_nothing_leaves`, over a real store and a shell line for the command `capital_a_then_r_on_a_row_writes_the_reply_with_the_configured_agent_command`, and the `agent-menu` goldens |
| `:agent link <n>` asks its question at once, hands the link and the redacted message to the command under `[agent]`, and shows its line as advice that opens nothing | `agent_link_asks_at_once_and_shows_what_comes_back_as_advice`, `the_agent_s_advice_arrives_under_a_question_already_asked` |
| `:agent reply` and `:agent tidy` hand text to the command under `[agent]` with names, addresses, numbers and links replaced by placeholders, and put them back in what returns | `the_table_of_what_is_replaced`, `agent_reply_sends_the_thread_redacted_and_restores_what_returns`, `agent_tidy_runs_the_configured_command_and_without_one_is_not_a_command` |
| A body an agent wrote or changed is marked as an agent's through to the summary and the saved draft, and `u` brings back your own | `an_agent_s_text_marks_the_draft_through_the_summary_and_the_saved_draft`, `agent_tidy_marks_what_it_changed_and_u_restores_the_user_s_text` |
| An agent command that fails, prints nothing, runs too long or is cancelled changes nothing | `a_command_that_fails_is_an_error_saying_how_and_never_what_it_printed`, `a_command_that_outlasts_its_time_is_killed_and_said_to_have`, `a_cancel_from_another_thread_kills_the_command`, `esc_cancels_the_agent_and_a_late_answer_changes_nothing` |
| Without an `[agent]` table there is no `:agent`, and nothing leaves | `without_an_agent_table_the_command_does_not_exist_and_nothing_leaves` |
| The `[agent]` line [`docs/config.example.toml`](https://github.com/lampseen/tuimex/blob/main/docs/config.example.toml) gives for the Claude CLI parses; whether that CLI then behaves as its help says is not tested | `the_example_config_parses_as_written_and_with_every_optional_key_set` |

## The subcommands for an outside agent

| what is true | what proves it |
|---|---|
| Every answer is decided by one function over the account's grants | `every_request_against_every_grant` |
| With no grant, every subcommand is refused and names the grant missing | `with_no_grant_anywhere_every_subcommand_is_refused` |
| An agent sees only the folders `read` covers, whatever it asks for | `list_returns_only_messages_in_folders_read_covers`, `search_finds_only_what_the_agent_may_read`, `thread_gives_the_conversation_less_what_read_does_not_cover` |
| The text an agent gets is the text the reader shows, and nothing else of the message | `show_gives_the_readers_text_for_every_corpus_message` |
| Text hidden in a message's HTML does not reach an agent | `show_on_the_hidden_instruction_returns_no_hidden_text` |
| `draft` files a draft marked as an agent's, with no attachment, and sends nothing | `draft_files_an_agents_draft_with_no_attachment_and_sends_nothing`, `draft_has_no_way_to_attach_and_refuses_what_it_cannot_do_whole` |
| There is no subcommand that sends, moves, deletes or flags | `the_binary_has_no_subcommand_outside_the_list` |
| Every call is a line in `~/.local/state/tuimex/audit.jsonl`, with no message text; a call that cannot be recorded answers nothing | `every_call_is_one_line_in_the_audit_log_with_no_message_text`, `a_call_that_cannot_be_recorded_answers_nothing` |
| No call prints or writes a secret put in the config by mistake | `no_agent_call_prints_or_writes_a_secret` |
| No subcommand can open a connection | `the_agent_module_cannot_reach_the_network` |
## What no test reaches

Each of these is built, and none is proven by anything that runs by
itself.

| what | where it stands |
|---|---|
| **Gmail, as Google runs it** | two accounts were set up and synced by hand on 2026-11-23. Which way each signed in, sending, and every other step were not recorded. The browser sign-in itself is tested only against a stand-in for Google |
| **Proton Bridge** | one account was set up and synced by hand on 2026-11-23, with nothing else recorded. Pinning is tested against a container's certificate, not Bridge's |
| **Fastmail and iCloud** | setup knows their servers. Nobody has run either |
| **The system keyring** | every test uses a keyring in memory. The Secret Service and the macOS keychain have been used only in the two by-hand runs above |
| **The real Claude CLI behind `use = "claude"`** | no test runs it, because running it sends text to a provider. A test holds the arguments tuimex gives it; that the program then behaves as its help says is not tested |
| **Sending from the interface through a real provider** | sending is run against servers in containers by `make real-server`, and against no provider's own |
| **macOS** | the CI job `macos` runs the whole suite on a Mac. Nobody has used tuimex on one by hand |
| **A picture drawn in a terminal** | tests check what is handed to the terminal's image protocol. None has a terminal to see the result on |
| **How a browser draws the sealed copy** | tests check what the page contains and what its policy allows. None opens a browser |
| **The system's opener** | tests check what would be handed to it, and never start a browser or a viewer |
| **A long mailbox** | drawing, filtering and start-up over 50,000 and 150,000 messages are measured by tests run by hand, not by `make check` |

The by-hand script for the first four is
[`docs/testing.md`](https://github.com/lampseen/tuimex/blob/main/docs/testing.md).
Nobody has run it step by step; its record at the end has the one row
above.

## Not built

OpenPGP (signing, encrypting, verifying, decrypting), answering an
invitation, the mouse, `NO_COLOR`, remapping keys under `[keys]`, new-mail
notifications, and `ui.glyphs` and `ui.colors` in the config. Each is
planned; none is claimed anywhere on these pages.

---

[Where it runs](Where-it-runs) · [Security and privacy](Security-and-privacy)
