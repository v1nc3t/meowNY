package com.meowny.server.controller;

import com.meowny.server.dto.recurringtransaction.CreateRecurringTransactionRequest;
import com.meowny.server.dto.recurringtransaction.RecurringTransactionResponse;
import com.meowny.server.dto.recurringtransaction.UpdateRecurringTransactionRequest;
import com.meowny.server.service.RecurringTransactionService;
import jakarta.validation.Valid;
import jakarta.validation.constraints.NotNull;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.validation.annotation.Validated;
import org.springframework.web.bind.annotation.*;

import java.util.List;

@RestController
@RequestMapping("/api/v1/recurring-transactions")
@Validated
public class RecurringTransactionController {

    private final RecurringTransactionService recurringTransactionService;

    public RecurringTransactionController(RecurringTransactionService recurringTransactionService) {
        this.recurringTransactionService = recurringTransactionService;
    }

    @GetMapping("/{id}")
    public ResponseEntity<RecurringTransactionResponse> getTemplateById(@PathVariable Long id) {
        RecurringTransactionResponse response = recurringTransactionService.getTemplateById(id);
        return ResponseEntity.ok(response);
    }

    @GetMapping
    public ResponseEntity<List<RecurringTransactionResponse>> getTemplatesByUserId(
            @RequestParam @NotNull Long userId) {

        List<RecurringTransactionResponse> responses = recurringTransactionService.getTemplatesByUserId(userId);
        return ResponseEntity.ok(responses);
    }

    @PostMapping
    public ResponseEntity<RecurringTransactionResponse> createTemplate(
            @Valid @RequestBody CreateRecurringTransactionRequest request) {

        RecurringTransactionResponse response = recurringTransactionService.createTemplate(request);
        return ResponseEntity.status(HttpStatus.CREATED).body(response);
    }

    @PutMapping("/{id}")
    public ResponseEntity<RecurringTransactionResponse> updateTemplate(
            @PathVariable Long id,
            @Valid @RequestBody UpdateRecurringTransactionRequest request) {

        RecurringTransactionResponse response = recurringTransactionService.updateTemplate(id, request);
        return ResponseEntity.ok(response);
    }

    @DeleteMapping("/{id}")
    public ResponseEntity<Void> deleteTemplate(@PathVariable Long id) {
        recurringTransactionService.deleteTemplate(id);
        return ResponseEntity.noContent().build();
    }
}