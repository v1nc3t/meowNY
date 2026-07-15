package com.meowny.server.dto.budget;

import jakarta.validation.constraints.*;

import java.math.BigDecimal;

public record CreateBudgetRequest(
        @NotNull(message = "User ID is required")
        Long userId,

        @NotNull(message = "Category ID is required")
        Long categoryId,

        @NotNull(message = "Limit amount is required")
        @PositiveOrZero(message = "Limit amount must be zero or positive")
        @Digits(integer = 12, fraction = 2)
        BigDecimal limitAmount,

        @NotNull(message = "Month is required")
        @Min(value = 1, message = "Month must be at least 1")
        @Max(value = 12, message = "Month must be at most 12")
        Integer month,

        @NotNull(message = "Year is required")
        @Min(value = 2000, message = "Year must be 2000 or later")
        @Max(value = 9999, message = "Year must be 9999 or earlier")
        Integer year
) {
}
