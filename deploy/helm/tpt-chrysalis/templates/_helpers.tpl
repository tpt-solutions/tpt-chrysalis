{{/*
Expand the name of the chart.
*/}}
{{- define "tpt-chrysalis.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "tpt-chrysalis.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "tpt-chrysalis.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "tpt-chrysalis.labels" -}}
helm.sh/chart: {{ include "tpt-chrysalis.chart" . }}
{{ include "tpt-chrysalis.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "tpt-chrysalis.selectorLabels" -}}
app.kubernetes.io/name: {{ include "tpt-chrysalis.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "tpt-chrysalis.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "tpt-chrysalis.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Create secret name for capability root key
*/}}
{{- define "tpt-chrysalis.capabilitySecretName" -}}
{{- printf "%s-capability-root-key" (include "tpt-chrysalis.fullname" .) }}
{{- end }}

{{/*
Create secret name for MinIO credentials
*/}}
{{- define "tpt-chrysalis.minioSecretName" -}}
{{- printf "%s-minio-credentials" (include "tpt-chrysalis.fullname" .) }}
{{- end }}

{{/*
Create TLS secret name
*/}}
{{- define "tpt-chrysalis.tlsSecretName" -}}
{{- printf "%s-tls" (include "tpt-chrysalis.fullname" .) }}
{{- end }}