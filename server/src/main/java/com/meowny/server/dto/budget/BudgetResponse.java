package com.meowny.server.dto.budget;

import java.math.BigDecimal;
import java.time.LocalDateTime;

public record BudgetResponse(
        Long id,
        Long userId,
        Long categoryId,
        String categoryName,
        BigDecimal limitAmount,
        Integer month,
        Integer year,
        LocalDateTime createdAt,
        LocalDateTime updatedAt
) {
}
