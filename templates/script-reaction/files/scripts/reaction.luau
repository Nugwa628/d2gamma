assert(SCRIPT_API_VERSION == 1)
return {
    react = function(e, api)
        if e.unit_type ~= 0 then return 0 end -- player messages only
        -- Trigger level is the holder's level stat, passed as an integer.
        api.message(e.unit, "Script reaction, level " .. tostring(e.argument))
        api.timer(e.unit, 25, e.argument)
        return 1 -- nonzero means this block acted
    end,
    on_timer = function(e, api)
        api.message(e.unit, "Script timer complete")
    end,
}
