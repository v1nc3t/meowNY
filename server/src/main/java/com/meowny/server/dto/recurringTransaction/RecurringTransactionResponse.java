package com.meowny.server.dto.recurringTransaction;

import com.meowny.commons.entity.Frequency;

import java.math.BigDecimal;
import java.time.LocalDate;
import java.time.LocalDateTime;

public record RecurringTransactionResponse(
        Long id,
        Long userId,
        Long categoryId,
        String categoryName,
        String name,
        BigDecimal amount,
        LocalDate nextDueDate,
        Frequency frequency,
        boolean isActive,
        LocalDateTime createdAt,
        LocalDateTime updatedAt
) {
}
