package com.meowny.server.dto.user;

import com.fasterxml.jackson.databind.annotation.JsonDeserialize;
import com.meowny.server.config.HtmlSanitizationDeserializer;
import jakarta.validation.constraints.Email;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Size;

public record UpdateUserRequest (
        @NotBlank(message = "First name is required")
        @Size(max = 50, message = "First name must be 50 characters or fewer")
        @JsonDeserialize(using = HtmlSanitizationDeserializer.class)
        String firstName,

        @NotBlank(message = "Last name is required")
        @Size(max = 30, message = "Last name must be 30 characters or fewer")
        @JsonDeserialize(using = HtmlSanitizationDeserializer.class)
        String lastName,

        @Email(message = "Must be a valid email format")
        @NotBlank(message = "Email is required")
        @Size(max = 254, message = "Email must be 254 characters or fewer")
        String email
) {}
