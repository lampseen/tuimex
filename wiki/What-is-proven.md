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
| The pointer is reported only when `ui.mouse = true`, which is off when left out, or from when `:mouse` is typed until it is typed again | `the_pointer_is_reported_only_when_the_config_asks_for_it`, `ui_mouse_is_off_unless_the_config_says_true`, `the_mouse_command_turns_the_pointer_on_and_off_and_alone_to_the_other`, `the_mouse_command_has_the_pointer_reported_from_then_until_it_is_typed_again` |
| With it on, a press moves the cursor, a second opens the row, and the pointer is given back before the screen is left, around `$EDITOR`, and in a crash | `a_press_moves_the_cursor_a_second_opens_the_row_and_the_pointer_is_given_back_on_the_way_out`, `the_editor_has_a_terminal_that_does_not_report_the_pointer`, `a_panic_gives_the_pointer_back_before_its_message_prints` |
| Only the left button going down and the wheel are read, with no modifier held, and in the order they came among the keys | `only_the_left_button_going_down_and_the_wheel_are_read_and_with_nothing_held`, `a_press_and_the_keys_around_it_are_handled_in_the_order_they_came` |

## Setup, signing in, and doctor

| what is true | what proves it |
|---|---|
| On a terminal with no config, `tuimex` runs setup: an account is added only once its IMAP and SMTP servers have taken the sign-in, a certificate the system does not trust is shown and, accepted, pinned for both; then the interface opens. `tuimex setup` adds further accounts after what the config held, which is kept to the byte | `setup_adds_two_accounts_from_nothing_and_sync_and_doctor_then_need_no_answers`, by `make real-server`; `a_first_launch_on_a_terminal_runs_setup_and_opens_nothing_when_setup_is_left`; `a_second_account_is_added_after_what_was_there_which_is_kept_to_the_byte` |
| A password typed into setup is not echoed, and is kept in the keyring or not at all | `setup_adds_two_accounts_from_nothing_and_sync_and_doctor_then_need_no_answers` (echo, and the refusal with no keyring service); `an_account_is_added_from_nothing_once_both_servers_have_taken_the_password`, on a keyring in memory. The system's keyring itself: no test |
| Gmail signs in through a browser with a client of your own: a loopback redirect, PKCE, the state checked, and on a machine with no browser the pasted address; two accounts share one client; an access token is replaced when it ends, and a revoked sign-in says to run setup | `two_gmail_accounts_sign_in_through_a_browser_and_share_one_client`, `with_no_browser_the_address_is_printed_and_the_pasted_redirect_or_code_is_taken`, `a_code_is_not_exchanged_by_a_sign_in_that_did_not_ask_for_it`, `a_token_that_ends_mid_session_is_replaced_unasked_and_a_revoked_sign_in_stops_naming_setup`, all against a stand-in for Google; `a_bearer_token_signs_in_over_imap_and_smtp_and_a_wrong_one_is_a_refused_sign_in`, by `make real-server`, against a server that takes the account's password as the token. Google itself: no test |
| `tuimex doctor` checks every account end to end and this machine, runs nothing it is only to find, and exits non-zero when something is wrong | `a_working_setup_is_reported_line_by_line_and_nothing_is_wrong`, `each_thing_that_is_broken_is_named_with_what_mends_it_and_the_run_fails`, `the_agent_command_is_found_and_never_run_and_a_custom_one_is_not_vouched_for`; by `make real-server`, in `setup_adds_two_accounts_from_nothing_and_sync_and_doctor_then_need_no_answers` |
| `tuimex doctor --redact` prints the same report line for line with every account name, address, user name, server host, certificate fingerprint and the home directory replaced by a placeholder that is the same wherever the value recurs, inside a server's own words too, and no name inside a longer word | `redact_replaces_each_kind_of_value_where_it_stands_whole_and_nothing_else`; over a working setup and each broken one in `a_working_setup_is_reported_line_by_line_and_nothing_is_wrong` and `each_thing_that_is_broken_is_named_with_what_mends_it_and_the_run_fails`; through the binary in `doctor_redacted_prints_the_same_report_with_no_name_address_host_or_home_in_it`. What the config does not name, such as a folder in a path: not replaced |
| No password, client secret or token that setup, the sign-in or doctor handled is in a file or in anything printed, with `--redact` or without | `a_secret_never_reaches_a_file_or_anything_setup_or_doctor_says`, a named CI job |

## Calendars

The calendar servers in the unit tests are stand-ins: they answer from a
script, or keep a table in memory with the preconditions a CalDAV server
keeps. Reading and writing over CalDAV are also run against a real
server, Radicale in a container, by
`an_event_is_written_changed_answered_and_deleted_on_a_caldav_server` and
`an_address_that_redirects_is_not_followed` under `make real-server`.

**Never run against the real thing:** Google's and Proton's own servers,
by any test; writing to Google's calendar, by anyone, by hand or by a
test; `notify-send` and `osascript`; and a Mac in a person's hands. The
table at the end of this page says where each stands.

### Reading, the config and setup

| what is true | what proves it |
|---|---|
| A `[[calendar]]` is `caldav` with an `https://` address and no user name in it, or a `feed` with no address in the config; one of no account, a second for one account, and each other mistake is an error naming its key, and none repeats a feed's address | `a_calendar_is_caldav_with_an_address_or_a_feed_with_none`, `a_calendar_table_that_is_wrong_says_which_key_and_never_quotes_a_feed_s_address`, `the_reference_in_the_plan_parses_with_every_optional_key_set` |
| Setup offers a Gmail account that signs in with an app password its calendar over CalDAV, tries that sign-in once before writing anything, and on a refusal says so and offers the secret address | `a_gmail_account_s_calendar_is_read_over_caldav_with_its_app_password`, `a_gmail_calendar_that_refuses_the_app_password_is_offered_by_its_secret_address` |
| Setup asks for a feed's address unseen, says where Proton and Google keep it, reads it once, saves nothing when it serves a web page or when no keyring will keep it, keeps it in the keyring alone, and asks again on the next run after a no | `a_feed_is_asked_for_unseen_checked_once_and_kept_in_the_keyring_alone`, `an_account_already_here_is_offered_a_calendar_and_what_cannot_be_kept_is_not_written` |
| A rename takes the account's `[[calendar]]` line, its feed's keyring entry and its own events with it | `a_rename_takes_the_account_s_calendar_and_its_feed_s_address_along` |
| `tuimex sync` reads each calendar after the mail, a line each with its kind and host; CalDAV signs in as the account's mail does; one that fails says why and the others are still read; `--account` limits it | `sync_says_a_line_for_each_calendar_and_a_feed_s_host_alone`, `each_calendar_is_signed_in_to_as_its_account_s_mail_or_read_from_its_feed`, `a_pass_over_two_accounts_reads_the_one_that_answers_when_the_other_fails` |
| CalDAV is asked for 90 days back to two years on, only what changed is fetched, nothing on another host is followed, and a calendar that fails keeps what it held | `caldav_is_asked_for_ninety_days_back_and_two_years_on`, `a_calendar_is_fetched_once_and_then_only_what_changed`, `a_server_cannot_add_what_was_not_listed_or_send_the_fetch_elsewhere`, `a_refusal_says_what_kind_and_changes_nothing`, `a_feed_is_kept_whole_and_its_address_is_in_no_error` |
| Only an `https://` address with no user name in it is fetched | `only_an_address_over_tls_with_no_sign_in_name_is_asked` |
| A new event is written to a CalDAV calendar only where nothing is yet, under a name tuimex made and never the event's own UID; a name that would leave the calendar is refused before anything is sent | `a_new_event_is_put_only_where_nothing_is_and_kept_with_the_tag_the_server_gave`, `a_new_event_s_name_is_tuimex_s_own_and_never_leaves_the_calendar` |
| A change or a delete is sent only over the version last read: if the event changed at the provider meanwhile, it is refused, said so, and nothing held here is touched; with no version held, nothing is sent until the calendar is read again | `a_change_is_put_only_over_the_tag_last_read_and_a_412_leaves_the_store_as_it_was`, `a_delete_is_conditional_and_one_already_gone_is_done`, `a_write_with_no_tag_held_is_refused_before_anything_is_sent_until_a_fetch_reads_it_again` |
| A write goes only to an event under the calendar's own address on its own server; a refused sign-in, a redirect and any other refusal are said by kind and change nothing | `an_address_off_the_calendar_is_refused_before_anything_is_sent`, `a_write_the_server_refuses_says_what_kind_and_changes_nothing`, `a_read_carries_no_precondition_and_says_its_body_is_xml` |
| `tuimex doctor` names each calendar's kind, its host, whether its password or address is found, and when it was last read, asking no calendar's server; `--redact` hides the host | `each_calendar_is_named_with_its_kind_its_host_its_secret_and_when_it_was_last_read` |
| A feed's address is in no file under the config, data, cache and state directories and in nothing setup, sync, doctor or the calendar's thread says, read or failing | `a_secret_never_reaches_a_file_when_a_calendar_feed_is_set_up_and_read`, which the CI job `secret` runs by its name; `a_feed_s_address_is_printed_by_its_command_and_a_failing_command_says_so_without_it` |
| A CalDAV calendar is written at its provider only where a table says `write = true`; left out it is false, a feed can never have it, and a table with no `kind` adds it to the account's table above. A server on this machine may be `http://127.0.0.1` and no other host may | `a_caldav_calendar_is_written_only_where_a_table_says_so_and_a_feed_never` |
| Setup says it will add one event named "tuimex write test" a year from today and remove it, asks, does so, and only then writes `write = true`; a no writes nothing | `a_calendar_is_written_once_setup_has_put_an_event_there_and_removed_it_again`, `a_gmail_account_s_calendar_is_read_over_caldav_with_its_app_password`, against a stand-in. Against Google: never run |
| A write that fails, or an event that was added and could not be removed, is said as it happened and leaves the calendar read-only | `a_write_that_fails_is_said_and_leaves_the_calendar_read_only` |
| For a calendar already in the config the check is on setup's menu, and what it found is added after everything that was there, kept to the byte | `a_calendar_already_in_the_config_is_tried_from_the_menu_and_the_key_is_added_after_what_was_there` |

### The views

| what is true | what proves it |
|---|---|
| `gc` opens the calendar on today and `gm` returns to mail, each side where it was; the calendar's keys are read before mail's; a command that takes rows of mail is refused there and says `gm` returns | `gc_opens_the_calendar_on_today_and_gm_returns_with_each_side_where_it_was`, `the_calendar_s_keys_are_read_before_the_ones_of_everywhere`, `the_calendar_has_its_commands_and_refuses_the_ones_that_take_rows_of_mail`, `a_session_in_the_calendar_moves_by_days_and_returns_to_mail` |
| `F2` opens the calendar and `F1` returns to mail wherever `gc` and `gm` do, and neither does anything where letters are typed; a terminal's two ways of sending each are read | `f2_opens_the_calendar_and_f1_returns_to_mail_wherever_gc_and_gm_do`, `a_function_key_is_one_press_and_not_its_letter_and_digit`, `the_keys_the_table_names_by_word_arrive_as_themselves`, `a_session_in_the_calendar_moves_by_days_and_returns_to_mail` |
| The month, the week, the day, an event's own screen, the form, the calendars panel, a find and the empty calendar are each drawn as the design draws them, cell for cell, at 120 by 40 and at 80 by 24 in ASCII | `cal_month`, `cal_week`, `cal_day`, `cal_event`, `cal_edit`, `cal_calendars`, `cal_find`, `cal_search`, `cal_empty`, `every_cell_is_written_at_any_size_wherever_the_calendar_is` |
| The week is drawn on a faint grid of days and hours, a step brighter under the selected day, whose header is filled; an event is a block behind an edge in its account's colour and is as bright when it has ended; the present is a line across today; what lasts all day stands over a rule; and every row fits at each width from 80 to 120 columns | `the_grid_is_faint_a_step_brighter_under_the_cursor_and_the_present_is_a_line_across_today`, `an_event_is_a_block_behind_its_account_s_edge_and_is_not_dimmed_when_it_has_ended`, `under_the_week_s_width_three_days_show_around_the_cursor_and_every_row_fits`, `cal_week` |
| The first row of every screen names mail and the calendar, each by its function key, and marks the one that is open in bold, with the other dim and nothing in the accent; where letters are typed and the keys do nothing it names no key; a row too narrow shortens the place not in view and then leaves its key alone; in the real binary the row is over the list, the calendar and an event | `the_bar_of_places_marks_the_place_in_view_and_names_its_keys_only_where_they_are_read`, `the_bar_names_each_place_by_its_function_key_and_marks_the_one_in_view`, `the_bar_draws_nothing_in_the_accent`, `the_bar_names_no_key_where_the_keys_are_not_read`, `a_narrow_bar_shortens_the_place_not_in_view_and_then_leaves_its_key`, `places`, `a_session_in_the_calendar_moves_by_days_and_returns_to_mail` |
| In the month `Alt-l` or `Alt-j` puts the focus in the list of events beside the grid and `Alt-h`, `Alt-k` or `Esc` returns it; the grid's cursor is dim meanwhile, and the screen is the design's at 120 and at 180 columns and at 80 by 24 in ASCII | `alt_l_focuses_the_month_s_list_of_events_and_alt_h_or_esc_returns_to_the_grid`, `the_focus_is_in_the_grid_after_another_view_and_only_the_month_has_a_list_to_focus`, `cal_month_list`, `a_session_in_the_calendar_moves_by_days_and_returns_to_mail` |
| The focus lands on an event whenever the list shows one: the first of the selected day, or from a day with nothing on it the first listed after, with the list left as it was, the row filled and the grid's cursor on that event's day; in the real binary `Esc` then `l`, `j` and `Enter` from an empty day open an event of a later day | `the_focus_lands_on_the_first_event_listed_when_the_cursor_s_day_has_none`, `cal_month_list`, `a_session_in_the_calendar_moves_by_days_and_returns_to_mail` |
| In the list `j` `k` `J` `K` step through every event it shows, across days, and the grid's cursor follows; `k` on the first event listed goes to the one before and the list begins there; the list scrolls to keep the selection in sight, goes on as far as there are events, and the grid changes month with it | `j_and_k_step_through_every_event_the_list_shows_and_the_grid_s_cursor_follows`, `k_at_the_top_of_the_list_goes_to_the_event_before_and_the_list_begins_on_its_day`, `the_list_scrolls_to_keep_the_selection_in_sight_and_the_month_changes_when_it_must`, `a_selected_event_further_down_its_day_than_the_rows_reach_is_the_last_row`, `cal_month_list` |
| In the list `h` `l` go to the day before or after that has an event; `Enter` opens the selected event and `o` `i` `E` `d` act on it; what is deleted or hidden leaves the selection to the next event listed; where the list shows no event the focus stays in the grid and the key says so | `h_and_l_in_the_list_are_the_day_before_and_after_that_has_an_event`, `enter_opens_the_selected_event_and_o_i_e_and_d_act_on_it`, `where_the_list_shows_no_event_the_focus_stays_in_the_grid_and_the_key_says_so`, `cal_month_list` |
| `h l j k [ ]` and `t` move the cursor, `Enter` goes deeper and `Esc` comes back, `J K` step through events across days, `gd` goes to a date and says what its words mean | `a_day_a_week_and_a_page_each_move_the_cursor_and_t_brings_it_back`, `enter_goes_deeper_and_esc_comes_back`, `events_are_stepped_through_across_days_in_the_order_they_are_drawn`, `gd_says_what_the_words_mean_and_goes_there`, `the_words_of_a_date_name_one_day` |
| `/` finds as it is typed and `Enter` lists every match; `I` goes round the invitations that wait; `gt` shows one account's events and leaves mail's narrowing alone; `C` hides a calendar from every view | `a_find_is_typed_and_enter_searches_every_day`, `capital_i_goes_round_the_invitations_that_wait_for_an_answer`, `gt_shows_one_account_s_events_and_leaves_mail_as_it_was`, `the_calendars_panel_hides_a_calendar_from_every_view` |
| A repeating event is on each of its days for the rules tuimex works out, keeps its hour when the clocks change, and is not drawn twice where a day is left out or stood in for; any other rule shows the event once and says so | `each_kind_of_rule_gives_its_days`, `a_repeat_keeps_its_hour_when_the_clocks_change`, `an_occurrence_left_out_or_replaced_is_not_drawn_twice`, `a_rule_tuimex_does_not_work_out_says_so` |
| A time is placed on the clock only when its zone is known, from the calendar and then from the system; otherwise it is shown as written | `a_written_time_is_placed_only_when_its_zone_is_known` |
| An event held by the provider and by a local answer is one event on screen, with the answer given here until the provider's copy has one | `the_same_uid_held_twice_is_one_event_with_the_answer_given_here_until_the_provider_has_one`, `an_answered_occurrence_joins_the_series_the_provider_holds` |
| A calendar kept on disk is shown whatever its provider says, and one that cannot be fetched says so with how old it is; why it could not be read is kept for `:messages`, once for each failure | `a_calendar_kept_on_disk_is_shown_and_one_that_cannot_be_fetched_says_so`, `why_a_calendar_could_not_be_read_is_kept_for_messages_once` |

### Answering an invitation

| what is true | what proves it |
|---|---|
| `i` asks the five answers of an event somebody asked you to, and says why for one you organize or were not asked to; the reader's card answers the event its message carries | `i_asks_the_five_answers_of_an_event_somebody_asked_the_reader_to`, `an_event_the_reader_organizes_or_was_not_asked_to_says_why_it_is_not_answered`, `the_reader_s_card_answers_the_event_its_message_carries`, `a_card_whose_event_was_not_handed_over_says_it_could_not_be_read` |
| Each answer is one mail to the organizer alone, from the invited address of the account the invitation came to; ignoring sends nothing; an address of another account is refused | `each_answer_is_one_mail_to_the_organizer_from_the_invited_identity`, `the_reply_goes_to_the_organizer_alone_however_many_are_invited`, `ignoring_sends_nothing`, `an_answer_from_an_address_of_another_account_is_refused` |
| The reply is a `REPLY` that carries the event's identity and the one answer, and a proposal a `COUNTER` with the new time | `a_reply_carries_the_event_s_identity_and_one_answer`, `a_counter_proposes_a_time_as_one_tentative_guest` |
| The reply waits in the Outbox of the account it came to, and `u` takes it back | `an_answer_waits_in_the_outbox_of_the_account_it_came_to_and_u_takes_it_back` |
| An answer to an event of a feed, or of a CalDAV calendar that is not written, is kept as a local copy and not one request reaches the provider; `u` removes the copy and takes the reply out of the Outbox | `an_answer_to_an_event_of_a_feed_is_a_local_copy_and_a_reply_and_u_takes_both_back`, `a_calendar_the_config_does_not_say_is_written_is_never_written` |
| A new event or an answer kept local for an account whose calendar is read over CalDAV and not written says that `tuimex setup` can try writing to it; for a feed, an account with no calendar, and a change to an event, the line is as it was | `an_event_kept_local_for_want_of_a_write_check_says_setup_can_try_one`, `an_answer_kept_local_for_want_of_a_write_check_says_setup_can_try_one`, `a_caldav_calendar_is_written_only_where_a_table_says_so_and_a_feed_never`, `cal_new` |
| In a written calendar the answer changes one value of the text the provider holds, conditional on the version last read; what the provider wrote and tuimex does not read is kept; `u` puts the text back; a calendar changed elsewhere is refused with what to do, and nothing is mailed | `an_answer_in_a_written_calendar_changes_one_line_at_the_provider_and_u_puts_the_text_back`, `an_answer_changes_one_line_of_the_provider_s_text` |
| `u` on the reply's own row in the Outbox takes the calendar entry back with it; a reply that has left stands, and `u` says so; mail the engine refuses leaves no entry and no draft | `mail_s_own_undo_of_the_reply_takes_the_calendar_entry_back_and_a_reply_that_left_stands`, `an_answer_whose_mail_the_engine_refuses_leaves_no_calendar_entry` |
| A time proposed and an invitation ignored are kept across runs, in a file beside the local events that only you can read; an answer outright settles a proposal | `a_time_proposed_and_an_invitation_ignored_are_the_reader_s_own_and_are_kept_across_runs`, `what_was_ignored_and_proposed_is_kept_across_runs_in_a_file_only_its_owner_reads` |
| That file, when it does not read, is said at the start and never written over; an answer that does not need it still works | `answers_that_could_not_be_read_are_said_and_never_written_over`, `a_file_that_does_not_read_is_said_and_not_started_over` |
| An invitation that is in an Inbox and in no calendar is on the calendar, stored nowhere, until it is answered or ignored; one its provider's calendar holds is one event; a later run reads only the messages that are new or still wait | `an_invitation_that_is_in_mail_alone_is_on_the_calendar_until_it_is_answered_or_ignored`, `an_invitation_its_provider_s_calendar_holds_is_one_event_and_one_for_a_written_calendar_is_shown_in_it`, `a_run_reads_the_messages_that_are_new_and_those_still_waiting_and_no_others` |
| A new version and a cancellation from the organizer are applied to an event held locally and said once; a cancellation from anyone else changes nothing; a guest's reply to an event you organize is kept and said | `an_update_and_a_cancellation_from_the_organizer_are_applied_once_and_a_stranger_s_change_nothing`, `what_a_later_message_may_change`, `a_guest_s_reply_to_an_event_organized_here_is_kept_and_said` |
| What cannot be done is refused in plain words and changes nothing: an event still being saved, one that has gone, an undo of something already taken back | `what_cannot_be_done_is_refused_in_plain_words_and_changes_nothing`, `a_refusal_puts_back_what_that_change_did_and_says_why` |
| The whole path, in the interface over a mail server in memory: the invitation is on the calendar from mail, `i a` answers it, the event is a local file, the reply is the Outbox's row, `u` takes both back | `an_invitation_in_mail_is_answered_in_the_calendar_and_u_takes_the_reply_and_the_entry_back` |
| The same in the binary on a terminal, over stores on disk | `an_invitation_is_answered_in_the_calendar_its_reply_waits_in_the_outbox_and_u_takes_both_back` |
| Against a real mail server and a real CalDAV server, for a calendar with `write = true`: the reply is read back from the organizer's Inbox and the event from the calendar's server | `an_invitation_is_answered_on_a_terminal_and_the_reply_and_the_event_are_read_back_from_the_servers`, by `make real-server`, against GreenMail and Radicale in containers; it passed there by hand on 2026-11-24. The `real-server` job runs the same script on every pull request |

### Events of your own, links and export

| what is true | what proves it |
|---|---|
| The form opens on the selected day, takes dates as words and says what they mean, refuses what cannot be saved with the reason, and asks before guests are mailed | `c_opens_a_new_event_at_nine_on_the_cursor_s_day_in_the_first_account`, `the_status_line_says_what_a_date_was_taken_to_mean_and_words_that_name_none_are_refused`, `each_refusal_on_save_says_why`, `guests_are_asked_about_first_and_only_y_saves` |
| In the form `h` and `l` choose on Calendar, All day, Repeats and Reminder as `Left` and `Right` do, and are letters in a field that is typed into; the hint names them only where they choose | `h_and_l_choose_on_a_field_that_is_chosen_from_and_are_letters_where_text_is_typed`, `tab_moves_through_the_fields_and_left_and_right_choose`, `cal_edit` |
| In Starts and Ends what is typed is merged with what the field held: a time alone keeps the date, a date alone the time, the end moves with the start, an empty field keeps what it held, and a time typed into an all-day event turns All day off; `2pm`, `1430` and `9` are times | `a_time_alone_keeps_the_date_and_a_date_alone_keeps_the_time`, `a_date_left_empty_keeps_what_it_held`, `a_time_typed_into_an_all_day_event_ends_its_being_all_day`, `a_time_is_read_by_the_twelve_hour_clock_too_and_alone_without_its_colon`, `an_end_before_the_start_is_refused`, `cal_when` |
| A new event is kept under a UID made here, as a local file or at a written provider under a name tuimex made, and `u` removes it | `a_new_event_is_kept_under_a_uid_made_here_locally_or_at_the_provider_and_u_removes_it` |
| An event is changed in place and moved between calendars, and `u` puts each back with the text it had | `an_event_is_changed_in_place_and_moved_to_another_calendar_and_u_puts_each_back` |
| One day of a repeating event is stood in for or left out, and the event stays; deleted whole, everything of it goes; `u` brings each back | `one_day_of_a_repeating_event_is_stood_in_for_or_left_out_and_the_event_stays`, `d_on_a_repeating_event_asks_and_this_one_leaves_the_other_days` |
| Guests are mailed the invitation only when you agreed, and a cancellation when the organizer deletes; an event with no guests to mail is not saved as one that mailed them | `guests_are_mailed_an_invitation_when_the_reader_agreed_and_a_cancellation_when_the_event_is_deleted`, `an_event_made_here_is_sent_to_its_guests_and_so_is_its_cancellation` |
| An event of a calendar that is only read is neither deleted nor changed, and the refusal says where to do it | `a_calendar_the_config_does_not_say_is_written_is_never_written` |
| In the binary on a terminal: an event made with the form is an owner-only file, is there after a restart, and is deleted and brought back | `an_event_made_with_the_form_is_there_after_a_restart_and_is_deleted_and_brought_back` |
| An event's link is listed with the host it really opens, a deceptive one asks first, and where it opens it is assessed again; the agent is asked as for a link in mail and opens nothing | `o_lists_the_selected_event_s_links_with_the_host_each_opens`, `a_deceptive_link_is_asked_about_and_opens_only_on_yes`, `an_event_s_link_is_assessed_again_where_it_opens_and_a_refusal_names_its_key`, `an_event_s_link_is_put_to_the_agent_with_its_names_redacted_and_answered_under_the_ask_s_serial` |
| `:export` writes every local event, and nothing a provider holds, to one new owner-only file and never over one | `the_local_events_are_exported_as_one_calendar_and_an_event_is_found_as_an_ask_names_it`, `the_local_events_are_written_to_a_new_file_and_never_over_one`, `several_events_are_one_calendar_that_defines_each_zone_once` |

## Reminders

No test has run `notify-send` or `osascript`: every test hands the
reminder to a notifier that records it, and nobody here has seen either
program raise one.

| what is true | what proves it |
|---|---|
| A reminder falls due at each of an event's own alarms, or `lead` before it; each occurrence of a repeat has its own; an all-day event has none without an alarm; a cancelled or declined event, and one in a zone nothing defines, has none; looks at the clock a step at a time see each once, also across the day the clocks go back | `each_rule_of_a_reminder_holds` |
| One that fell due while nothing was looking is not raised late: after a gap of over five minutes, only what fell due in the last five, for an event not begun | `each_rule_of_a_reminder_holds`, `off_stays_off_across_a_restart_and_raises_nothing_and_no_backlog_when_turned_on` |
| A reminder is raised once, with the event's title and nothing else of it | `a_reminder_is_raised_once_with_its_title_alone_and_handed_to_the_interface` |
| What falls due is on the status line, in mail and in the calendar, until the event begins; the bell is written once, after the frame that shows it and never inside one | `what_falls_due_is_on_the_status_line_and_rings_the_bell_once`, `the_bell_is_written_once_after_the_frame_that_shows_what_fell_due`, `a_reminder_shows_in_mail_and_in_the_calendar_until_the_event_begins` |
| A calendar you hid and an invitation you ignored remind nobody, and neither does an event the account that holds it declined | `a_hidden_calendar_and_an_ignored_invitation_remind_nobody`, `an_event_is_declined_by_the_account_that_holds_it_and_by_no_other` |
| `gn` and `:notify` switch reminders, and off is what the status line says | `gn_turns_reminders_off_and_on_and_off_is_what_the_status_line_says`, `nothing_is_raised_while_off_and_the_switch_is_written_when_the_interface_s_changes` |
| The title loses its control and direction characters and is cut to 120 characters; an escape in it cannot end the terminal's OSC 9 early | `a_hostile_title_is_one_line_with_nothing_to_steer_text_or_a_terminal`, `osc9_writes_the_cleaned_title_and_an_escape_in_it_cannot_end_the_sequence_early` |
| `notify-send` is given the title as one argument after `--`, as its summary, which is plain text; `osascript` is given it as an argument to a fixed script | `the_title_is_one_argument_after_the_options_end_and_never_markup_or_script` (the argument list only: neither program is run) |
| A notification program that cannot start is said once, naming it, and reminders still reach the interface | `a_program_that_is_not_there_is_a_sentence_naming_it`, `a_notifier_that_fails_is_said_once_and_reminders_still_reach_the_interface` |
| Off stays off across a restart, whatever `events` says; a missing or unreadable switch file is `events`; the file is not read or written through a link | `off_stays_off_across_a_restart_and_raises_nothing_and_no_backlog_when_turned_on`, `the_switch_is_remembered_and_a_missing_or_strange_file_is_the_config_s_default`, `a_switch_file_that_is_a_link_is_not_read_and_not_written_through` |
| `[notify]`'s `events`, `lead` and `via` have the defaults [Configuration](Configuration) gives, and a wrong word is an error naming the key | `notify_s_keys_have_their_defaults_and_a_wrong_word_is_named` |

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
| With the mouse on, a press on a row moves the cursor there and a second opens it; a press on a date label, a blank line, the rule, the status line or the hint line does nothing | `a_click_on_a_row_moves_the_cursor_there_and_a_second_click_opens_it`, `a_click_anywhere_in_a_row_is_on_that_row_and_past_its_last_cell_is_on_none`, `a_click_on_a_date_label_a_blank_line_or_the_rule_does_nothing`, `a_click_on_the_hint_line_or_the_status_line_does_nothing` |
| A press on a tab opens that section, on a place goes there where its function key is read, and on the preview moves the focus into it | `a_click_on_a_tab_opens_that_section_and_on_the_open_one_does_nothing`, `a_click_on_a_place_goes_there_as_its_function_key_does`, `the_bar_is_not_pressed_where_its_keys_are_not_read`, `a_click_on_the_preview_takes_the_focus_there_and_on_a_row_gives_it_back` |
| The wheel moves the cursor over the rows and scrolls the preview over that; while a line is typed the pointer does nothing, while a question is open it does nothing but answer it, and a press ends a key sequence that was begun | `the_wheel_moves_the_cursor_over_the_list_and_scrolls_the_preview_over_it`, `a_press_off_a_question_s_choices_or_while_a_line_is_typed_does_nothing`, `a_press_ends_a_sequence_that_was_begun` |
| What a press finds on a cell is what was drawn there, at every size | `every_spot_lies_on_the_text_it_names_at_any_size`, `a_spot_is_the_cells_of_its_span_counted_as_a_terminal_counts_them`, `a_cell_is_what_was_last_drawn_on_it_and_nothing_where_nothing_was` |

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
| With the mouse on, a press on a `[n]` in the text lists the links with that one chosen and opens nothing; a press on an entry of the links or the files chooses it, and a second does what Enter does, so a link is assessed and asked about as a typed one is; the wheel scrolls the reader, help and `:messages` as their keys do | `a_click_on_a_link_reference_lists_the_links_and_opens_nothing`, `a_clicked_link_is_assessed_as_a_typed_one_is`, `a_click_on_a_file_moves_the_cursor_and_a_second_does_what_enter_does`, `the_wheel_scrolls_the_reader_help_and_messages` |
| With the mouse on, a press on a question's choice is that choice's key, and a press anywhere else leaves the question open | `a_question_is_answered_by_a_click_on_its_choice_and_a_click_elsewhere_leaves_it_open` |
| An invitation's card answers once the message has been read and its event handed over, and only an invitation to answer is: a cancellation or a reply has no event to answer. [Calendars](#answering-an-invitation) has what an answer then does | `a_message_read_tells_the_card_its_event_when_it_is_an_invitation_to_answer`, `an_invitation_s_card_in_the_reader_answers_once_its_message_has_been_read`, `the_reader_s_card_answers_the_event_its_message_carries`, `a_card_whose_event_was_not_handed_over_says_it_could_not_be_read` |

## Writing and sending

| what is true | what proves it |
|---|---|
| `c`, `r`, `R` and `f` open compose from the list and the reader, new or quoting the message as mutt does, with the headers that keep a reply in its thread; on a saved draft they go on writing it | `compose_opens_from_the_keys_and_the_commands_alike`, `a_reply_from_the_reader_quotes_what_the_reader_shows`, `a_reply_quotes_the_message_under_who_wrote_it_and_when`, `a_reply_carries_the_headers_that_keep_it_in_its_thread`, `a_forward_is_the_message_between_two_markers_and_answers_nothing` |
| The identity is chosen on From, by `h` and `l` or the arrows, and the draft is filed and sent under that identity's account; in a field that is typed into `h` and `l` are letters, and the hint names them only on From | `left_and_right_choose_the_identity_and_the_account_follows_it`, `tab_and_shift_tab_walk_the_fields_and_enter_moves_on_until_the_body` |
| `Enter` on the body opens `$EDITOR` on an owner-only file under the cache, and the interface comes back around it | `the_body_is_written_in_editor_and_the_interface_comes_back_around_it`, `the_body_goes_through_the_editor_in_an_owner_only_file_that_is_then_removed` |
| `^a` attaches a file, completing its path | `ctrl_a_attaches_a_file_with_the_path_completed`, `a_path_completes_to_the_files_it_could_name` |
| An address that does not parse stops the summary and is named | `an_address_that_does_not_parse_stops_the_summary_and_is_named` |
| The send summary lists every recipient on a line and marks one whose domain is nowhere in the thread; only `y` sends | `a_recipient_from_outside_the_thread_is_marked_by_registrable_domain`, `only_y_sends_and_the_draft_is_handed_on_before_the_send` |
| `^d` saves the draft, and `Esc` asks whether to keep one that changed | `ctrl_d_saves_the_draft_into_drafts_and_hands_it_on`, `esc_asks_keep_or_discard_only_when_something_changed`, and on a pseudo-terminal `a_session_writes_a_message_reads_its_summary_and_saves_the_draft` |
| With the mouse on, a press on a field of compose moves the focus there as Tab does, and types nothing and starts no editor; the send summary's and the keep-or-discard question's choices take a press as their keys | `a_click_on_a_compose_field_moves_the_focus_there`, `a_question_is_answered_by_a_click_on_its_choice_and_a_click_elsewhere_leaves_it_open` |
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

## Releases and dependencies

| what is true | what proves it |
|---|---|
| A release builds four archives, for Linux and macOS on x86_64 and arm64, under the names [Installing](Installing) gives | [`release.yml`](https://github.com/lampseen/tuimex/blob/main/.github/workflows/release.yml), which built, packaged and published all four for `v0.1.0` on 2026-11-24, in its jobs `build` and `release` ([run 45454590282](https://github.com/lampseen/tuimex/actions/runs/45454590282)); `the_release_builds_exactly_the_four_targets_and_none_for_windows`, `every_archive_the_readme_and_the_wiki_name_is_one_the_release_makes` |
| A published release carries `checksums.txt` and an attestation covering every archive, and the attestation ties each file to this repository's release workflow at the release's tag | the attest and publish steps of the job `release` in [`release.yml`](https://github.com/lampseen/tuimex/blob/main/.github/workflows/release.yml), which ran for the first time for `v0.1.0` on 2026-11-24 and passed ([run 45454590282](https://github.com/lampseen/tuimex/actions/runs/45454590282)). No test holds it; it was checked by hand that day on Linux x86-64, for `tuimex_linux_amd64.tar.gz` downloaded from the `latest` link: `sha256sum -c` against `checksums.txt` said OK; `gh attestation verify` exited 0 for the archive and for `checksums.txt`, naming SLSA provenance v1, all four archives and `checksums.txt` as subjects, and `.github/workflows/release.yml` at `refs/tags/v0.1.0`; a copy with one byte appended failed it; and the unpacked binary printed `tuimex 0.1.0`. The other three archives were not checked beyond their links answering |
| A dependency that has a newer release, or that an advisory names, is reported in an issue each week, and never changed by itself | [`outdated.yml`](https://github.com/lampseen/tuimex/blob/main/.github/workflows/outdated.yml), which has run on GitHub once, started by hand on 2026-11-24, and opened the issue for updates; `the_weekly_check_runs_when_the_plan_says`. Its schedule has not fired yet, and no advisory has yet been there for it to report |

## What no test reaches

Each of these is built, and none is proven by anything that runs by
itself.

| what | where it stands |
|---|---|
| **Gmail, as Google runs it** | two accounts were set up and synced by hand on 2026-11-23. Which way each signed in, sending, and every other step were not recorded. The browser sign-in itself is tested only against a stand-in for Google |
| **Proton Bridge** | one account was set up and synced by hand on 2026-11-23, with nothing else recorded. Pinning is tested against a container's certificate, not Bridge's |
| **Fastmail and iCloud** | setup knows their servers. Nobody has run either |
| **Calendars at Google and Proton** | no test fetches from either. By hand on 2026-11-24, two Google accounts were read over CalDAV with their app passwords and one Proton calendar from its private address, and their owner said the calendar looked right; nothing was compared event by event, and no feed from Google has been read. Whether Gmail has already put an invitation in the calendar by the time its mail is read, and so which of the two tuimex shows first, has not been observed |
| **Writing to Google's calendar** | no test does it. By hand on 2026-11-24, on two Google accounts: `tuimex setup`'s write check added its event and removed it and set `write = true`; an event made in tuimex was then seen in Google Calendar by its owner; and after he deleted it there, `:sync` removed it here. Changing an event, answering an invitation and deleting from tuimex have not been tried against Google. Google files a new event under an address of its own and names no tag for it, which `an_event_the_server_filed_under_its_own_address_is_kept_under_that_address` plays against a stand-in |
| **A `COUNTER` at a real provider** | a proposal is sent as one, with the same proposal as a sentence. Which providers act on the calendar part and which show only the sentence has not been tried |
| **The system keyring** | every test uses a keyring in memory. The Secret Service and the macOS keychain have been used only in the two by-hand runs above |
| **The real Claude CLI behind `use = "claude"`** | no test runs it, because running it sends text to a provider. A test holds the arguments tuimex gives it; that the program then behaves as its help says is not tested |
| **Sending from the interface through a real provider** | sending is run against servers in containers by `make real-server`, and against no provider's own |
| **macOS** | the CI job `macos` runs the whole suite on a Mac, and it has passed on `macos-latest` as of 2026-11-24. Nobody has used tuimex on one by hand |
| **The macOS archives** | both are published, and the release workflow checked that each binary is built for its architecture. Neither has been unpacked or run by anyone. That a download made with `curl` is not stopped by Gatekeeper, as [Installing](Installing) says, is how macOS is documented to behave and has not been tried with this binary |
| **The Linux arm64 archive** | built on an arm64 runner and published. It has not been run on an arm64 machine: the workflow builds and packages the binary and does not start it |
| **The Homebrew formula** | a template whose version and checksums are still placeholders. There is no tap, and nothing has installed from it |
| **`make release VERSION=…`** | the step that opens the pull request bumping the version has never been run for a real release: 0.1.0 needed no bump, and was cut by `make release-tag` alone |
| **A picture drawn in a terminal** | tests check what is handed to the terminal's image protocol. None has a terminal to see the result on |
| **How a browser draws the sealed copy** | tests check what the page contains and what its policy allows. None opens a browser |
| **`notify-send` and `osascript`** | no test runs either: tests check the argument list each would be given. That a notification service shows the summary as plain text is what the freedesktop specification says, not something a test has seen |
| **The system's opener** | tests check what would be handed to it, and never start a browser or a viewer |
| **A long mailbox** | drawing, filtering and start-up over 50,000 and 150,000 messages are measured by tests run by hand, not by `make check` |

The by-hand script for the first four is
[`docs/testing.md`](https://github.com/lampseen/tuimex/blob/main/docs/testing.md).
Nobody has run it step by step; its record at the end has the one row
above.

## Not built

OpenPGP (signing, encrypting, verifying, decrypting), the mouse in the
calendar, `NO_COLOR`, remapping keys under `[keys]`, new-mail notifications, and
`ui.glyphs` and `ui.colors` in the config. Each is planned; none is
claimed anywhere on these pages.

---

[Where it runs](Where-it-runs) · [Security and privacy](Security-and-privacy)
