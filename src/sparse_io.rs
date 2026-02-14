use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};

use drill_press::{ScanError, Segment, SegmentType};

pub fn sparse_error_brief(err: &ScanError) -> String {
    match err {
        ScanError::UnsupportedPlatform => "unsupported platform".to_string(),
        ScanError::UnsupportedFileSystem => "unsupported filesystem sparse API".to_string(),
        ScanError::IO(ioe) => ioe.to_string(),
    }
}

pub fn visit_sparse_segments<C, FData, FHole>(
    file: &mut File,
    segments: &[Segment],
    context: &mut C,
    mut on_data: FData,
    mut on_hole: FHole,
) -> io::Result<()>
where
    C: ?Sized,
    FData: FnMut(&mut C, &mut dyn Read) -> io::Result<()>,
    FHole: FnMut(&mut C, u64, u64) -> io::Result<()>,
{
    for segment in segments {
        if segment.range.is_empty() {
            continue;
        }

        let len = segment.range.end - segment.range.start;
        match segment.segment_type {
            SegmentType::Hole => on_hole(context, segment.range.start, len)?,
            SegmentType::Data => {
                file.seek(SeekFrom::Start(segment.range.start))?;
                let limited = (&mut *file).take(len);
                let mut reader = BufReader::new(limited);
                on_data(context, &mut reader)?;
            }
        }
    }

    Ok(())
}
