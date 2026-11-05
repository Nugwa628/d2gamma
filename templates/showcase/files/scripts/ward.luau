return {
    ward = function(e, api)
        api.apply_state(e.unit, e.unit, 186, 120, e.skill)
        api.ui_variable(e.unit, "ward_active", 1)
        api.ui_variable(e.unit, "casts", api.base_stat(e.unit, 40000, 0))
        return 1
    end,
    expired = function(e, api)
        api.ui_variable(e.unit, "ward_active", 0)
        return 1
    end,
    can_charge = function(current)
        return current < 20 and 1 or 0
    end,
}
