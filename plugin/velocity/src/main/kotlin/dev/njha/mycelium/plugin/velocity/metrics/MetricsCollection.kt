package dev.njha.mycelium.plugin.velocity.metrics

import dev.cubxity.plugins.metrics.api.metric.collector.Collector
import dev.cubxity.plugins.metrics.api.metric.collector.CollectorCollection


class MetricsCollection(collector: MetricsCollector) : CollectorCollection {
    override val collectors: List<Collector> = listOf<Collector>(collector)
}
