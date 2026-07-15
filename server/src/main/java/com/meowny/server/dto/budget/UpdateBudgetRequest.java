package com.meowny.server.dto.budget;

import jakarta.validation.constraints.Digits;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.PositiveOrZero;

import java.math.BigDecimal;

public record UpdateBudgetRequest(
        @NotNull(message = "Limit amount is required")
        @PositiveOrZero(message = "Limit amount must be zero or positive")
        @Digits(integer = 12, fraction = 2)
        BigDecimal limitAmount
) {
}
