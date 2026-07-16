return {
  id = "tracker_url_remover",
  name = "Tracker URL remover",
  description = "Remove trackers arguments from the returned URL",
  api_version = 1,
  kind = "result_plugin",
  default_enabled = true,
  preference_section = "privacy",
  order = 60,
  capabilities = {"result", "utils"},
  on_result = function(result, query, ctx)
    if result.url then result.url = ctx.utils.clean_url(result.url) end
    if result.img_src then result.img_src = ctx.utils.clean_url(result.img_src) end
    if result.thumbnail_src then result.thumbnail_src = ctx.utils.clean_url(result.thumbnail_src) end
    if result.thumbnail then result.thumbnail = ctx.utils.clean_url(result.thumbnail) end
    return true
  end,
}
