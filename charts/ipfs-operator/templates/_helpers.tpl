{{- define "controller.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "controller.fullname" -}}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- $name | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "controller.labels" -}}
{{- include "controller.selectorLabels" . }}
app.kubernetes.io/name: {{ include "controller.name" . }}
app.kubernetes.io/version: {{ .Values.images.controller.tag | default .Chart.AppVersion | quote }}
{{- end }}

{{- define "controller.selectorLabels" -}}
app: {{ include "controller.name" . }}
{{- end }}

{{- define "controller.image" -}}
{{- .Values.images.controller.repository }}
{{- end }}

{{- define "controller.tag" -}}
{{- if .Values.tracing.enabled }}
{{- "otel-" }}{{ .Values.version | default .Chart.AppVersion }}
{{- else }}
{{- .Values.version | default .Chart.AppVersion }}
{{- end }}
{{- end }}

{{- define "web.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}-web
{{- end }}

{{- define "web.fullname" -}}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- $name | trunc 63 | trimSuffix "-" }}-web
{{- end }}

{{- define "web.labels" -}}
{{- include "web.selectorLabels" . }}
app.kubernetes.io/name: {{ include "web.name" . }}
app.kubernetes.io/version: {{ .Values.images.web.tag | quote }}
{{- end }}

{{- define "web.selectorLabels" -}}
app: {{ include "web.name" . }}
{{- end }}

{{- define "web.image" -}}
{{- .Values.images.web.repository }}
{{- end }}

{{- define "web.tag" -}}
{{- .Values.images.web.tag }}
{{- end }}
